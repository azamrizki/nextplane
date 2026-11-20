//! Live co-editing of pages: one Yjs document per page in memory ("room"), shared by every
//! open socket, speaking the y-websocket protocol (sync + awareness). The document is saved
//! at most every two seconds while it changes, and once more when the last person leaves.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message as WsMsg, WebSocket};
use np_core::id::Uuid;
use np_store::Store;
use tokio::sync::broadcast;
use yrs::block::ClientID;
use yrs::sync::{Awareness, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

/// Sender id of a "close and reconnect" signal.
const KICK: u64 = 0;

pub struct Room {
    id: Uuid,
    project: Uuid,
    awareness: Mutex<Awareness>,
    tx: broadcast::Sender<(u64, Arc<Vec<u8>>)>,
    dirty: AtomicBool,
    conns: AtomicUsize,
    last_editor: Mutex<Option<Uuid>>,
}

#[derive(Default)]
pub struct Rooms {
    map: Mutex<HashMap<Uuid, Arc<Room>>>,
}

impl Rooms {
    /// The room of a page, loading its document if nobody has it open.
    pub async fn open(
        self: &Arc<Self>,
        store: &Store,
        id: Uuid,
        project: Uuid,
    ) -> anyhow::Result<Arc<Room>> {
        if let Some(r) = self.map.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
            r.conns.fetch_add(1, Ordering::SeqCst);
            return Ok(r.clone());
        }
        let saved = store.page_ydoc(id).await.map_err(|e| anyhow::anyhow!(e))?;
        let doc = Doc::new();
        if let Some(bytes) = saved.filter(|b| !b.is_empty()) {
            let update = Update::decode_v1(&bytes)?;
            doc.transact_mut().apply_update(update)?;
        }
        let room = {
            let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
            // Someone may have opened it while we were loading.
            if let Some(r) = map.get(&id) {
                r.conns.fetch_add(1, Ordering::SeqCst);
                return Ok(r.clone());
            }
            let room = Arc::new(Room {
                id,
                project,
                awareness: Mutex::new(Awareness::new(doc)),
                tx: broadcast::channel(1024).0,
                dirty: AtomicBool::new(false),
                conns: AtomicUsize::new(1),
                last_editor: Mutex::new(None),
            });
            map.insert(id, room.clone());
            room
        };
        tokio::spawn(saver(self.clone(), store.clone(), room.clone()));
        Ok(room)
    }

    /// Closes every socket on a page so the clients reconnect and their rights are checked
    /// again (after lock, archive, access change or delete).
    pub fn kick(&self, id: Uuid) {
        if let Some(r) = self.map.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
            let _ = r.tx.send((KICK, Arc::new(Vec::new())));
        }
    }

    /// After membership or project changes: every page of the project (or of every project
    /// when `None`) reconnects and is checked again.
    pub fn kick_project(&self, project: Option<Uuid>) {
        for r in self.map.lock().unwrap_or_else(|e| e.into_inner()).values() {
            if project.is_none_or(|p| p == r.project) {
                let _ = r.tx.send((KICK, Arc::new(Vec::new())));
            }
        }
    }

    pub fn open_rooms(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

impl Room {
    fn state(&self) -> Vec<u8> {
        let a = self.awareness.lock().unwrap_or_else(|e| e.into_inner());
        a.doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    async fn save(&self, store: &Store) {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        let by = *self.last_editor.lock().unwrap_or_else(|e| e.into_inner());
        let (bytes, html) = {
            let a = self.awareness.lock().unwrap_or_else(|e| e.into_inner());
            // One transaction at a time: yrs blocks on a write while a read is open.
            let html = crate::pagehtml::doc_html(a.doc());
            let bytes = a
                .doc()
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            (bytes, html)
        };
        if let Err(e) = store.save_page_doc(self.id, &bytes, &html, by).await {
            self.dirty.store(true, Ordering::SeqCst);
            tracing::error!(page = %self.id, error = %e, "page save failed");
        }
    }
}

async fn saver(rooms: Arc<Rooms>, store: Store, room: Arc<Room>) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    loop {
        tick.tick().await;
        room.save(&store).await;
        let gone = {
            let mut map = rooms.map.lock().unwrap_or_else(|e| e.into_inner());
            if room.conns.load(Ordering::SeqCst) == 0 && !room.dirty.load(Ordering::SeqCst) {
                map.remove(&room.id);
                true
            } else {
                false
            }
        };
        if gone {
            return;
        }
    }
}

/// Serves one socket until it closes. `can_write` = false makes the connection read-only
/// (its document updates are ignored; presence still works).
pub async fn serve(
    store: Store,
    room: Arc<Room>,
    mut socket: WebSocket,
    user: Uuid,
    can_write: bool,
) {
    let conn: u64 = rand_u64();
    let mut rx = room.tx.subscribe();
    let mut my_clients: HashSet<ClientID> = HashSet::new();

    // Start: our state vector (so the client sends what we miss), then current presence.
    // The y-websocket client reads one message per frame, so these are two frames.
    let hello: Vec<Vec<u8>> = {
        let a = room.awareness.lock().unwrap_or_else(|e| e.into_inner());
        let sv = a.doc().transact().state_vector();
        let mut frames = vec![Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1()];
        if let Ok(u) = a.update()
            && !u.clients.is_empty()
        {
            frames.push(Message::Awareness(u).encode_v1());
        }
        frames
    };
    for f in hello {
        if socket.send(WsMsg::Binary(f.into())).await.is_err() {
            room.conns.fetch_sub(1, Ordering::SeqCst);
            return;
        }
    }

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let data = match msg {
                    Some(Ok(WsMsg::Binary(b))) => b,
                    Some(Ok(WsMsg::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => continue,
                };
                let replies = handle(&room, &data, conn, user, can_write, &mut my_clients);
                let mut dead = false;
                for r in replies {
                    if socket.send(WsMsg::Binary(r.into())).await.is_err() {
                        dead = true;
                        break;
                    }
                }
                if dead { break; }
            }
            ev = rx.recv() => match ev {
                Ok((KICK, _)) => break,
                Ok((from, bytes)) if from != conn => {
                    if socket.send(WsMsg::Binary(bytes.as_ref().clone().into())).await.is_err() { break; }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Fell behind: send the whole document again (merging is idempotent).
                    let full = Message::Sync(SyncMessage::Update(room.state())).encode_v1();
                    if socket.send(WsMsg::Binary(full.into())).await.is_err() { break; }
                }
                Err(_) => break,
            },
        }
    }

    // Remove this connection's cursors for everyone else.
    if !my_clients.is_empty() {
        let bytes = {
            let a = room.awareness.lock().unwrap_or_else(|e| e.into_inner());
            for c in &my_clients {
                a.remove_state(*c);
            }
            a.update_with_clients(my_clients.iter().copied())
                .ok()
                .map(|u| Message::Awareness(u).encode_v1())
        };
        if let Some(b) = bytes {
            let _ = room.tx.send((conn, Arc::new(b)));
        }
    }
    room.conns.fetch_sub(1, Ordering::SeqCst);
    room.save(&store).await;
}

/// Applies one frame from a client; returns frames to send back to it.
fn handle(
    room: &Room,
    data: &[u8],
    conn: u64,
    user: Uuid,
    can_write: bool,
    mine: &mut HashSet<ClientID>,
) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut decoder = DecoderV1::from(data);
    let reader = MessageReader::new(&mut decoder);
    for msg in reader {
        let Ok(msg) = msg else { break };
        match msg {
            Message::Sync(SyncMessage::SyncStep1(sv)) => {
                let a = room.awareness.lock().unwrap_or_else(|e| e.into_inner());
                let diff = a.doc().transact().encode_state_as_update_v1(&sv);
                out.push(Message::Sync(SyncMessage::SyncStep2(diff)).encode_v1());
            }
            Message::Sync(SyncMessage::SyncStep2(u)) | Message::Sync(SyncMessage::Update(u)) => {
                if !can_write {
                    continue;
                }
                let Ok(update) = Update::decode_v1(&u) else {
                    continue;
                };
                {
                    let a = room.awareness.lock().unwrap_or_else(|e| e.into_inner());
                    if a.doc().transact_mut().apply_update(update).is_err() {
                        continue;
                    }
                }
                room.dirty.store(true, Ordering::SeqCst);
                *room.last_editor.lock().unwrap_or_else(|e| e.into_inner()) = Some(user);
                let _ = room.tx.send((
                    conn,
                    Arc::new(Message::Sync(SyncMessage::Update(u)).encode_v1()),
                ));
            }
            Message::Awareness(update) => {
                mine.extend(update.clients.keys().copied());
                let bytes = Message::Awareness(update.clone()).encode_v1();
                let ok = room
                    .awareness
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .apply_update(update)
                    .is_ok();
                if ok {
                    let _ = room.tx.send((conn, Arc::new(bytes)));
                }
            }
            Message::AwarenessQuery => {
                if let Ok(u) = room
                    .awareness
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .update()
                {
                    out.push(Message::Awareness(u).encode_v1());
                }
            }
            Message::Auth(_) | Message::Custom(..) => {}
        }
    }
    out
}

fn rand_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("OS random source");
    u64::from_le_bytes(b).max(1)
}
