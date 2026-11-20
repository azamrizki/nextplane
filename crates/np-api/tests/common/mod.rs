//! Test harness: a router on a throwaway data folder, and small request helpers.
#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use np_api::{AppState, VersionInfo, router};
use np_store::Store;
use serde_json::Value;
use tower::ServiceExt;

pub struct TestApp {
    pub app: Router,
    pub store: Store,
    pub events: np_api::events::Bus,
    _dir: tempfile::TempDir,
}

pub struct Res {
    pub status: StatusCode,
    pub body: Value,
    pub set_cookie: Option<String>,
}

impl Res {
    /// `name=value` of the session cookie this response set.
    pub fn cookie(&self) -> String {
        self.set_cookie
            .as_deref()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }
}

pub async fn app() -> TestApp {
    app_with(|_| {}).await
}

/// Like `app`, with a hook to adjust the server state (e.g. turn on single sign-on).
pub async fn app_with(adjust: impl FnOnce(&mut AppState)) -> TestApp {
    // Server-side errors (the cause of a 500) show in the output of a failing test.
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("warn")
        .try_init();
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = Store::open(dir.path(), &dir.path().join("backups"))
        .await
        .unwrap();
    let version = VersionInfo {
        version: "1.2.3".into(),
        commit: "abc1234".into(),
        build_date: "2026-11-20T00:00:00Z".into(),
    };
    let mut state = AppState::new(store.clone(), version);
    state.uploads_dir = dir.path().join("uploads");
    state.max_upload = 64 * 1024;
    adjust(&mut state);
    let events = state.events.clone();
    let app = router(state, Router::new());
    TestApp {
        app,
        store,
        events,
        _dir: dir,
    }
}

impl TestApp {
    pub async fn send(&self, req: Request<Body>) -> Res {
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let set_cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        Res {
            status,
            body,
            set_cookie,
        }
    }

    pub async fn get(&self, path: &str, cookie: Option<&str>) -> Res {
        let mut b = Request::get(path);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        self.send(b.body(Body::empty()).unwrap()).await
    }

    pub async fn post(&self, path: &str, body: Value, cookie: Option<&str>) -> Res {
        let mut b = Request::post(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::HOST, "localhost:12313")
            .header(header::ORIGIN, "http://localhost:12313");
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        self.send(b.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// Runs setup with a fresh token; returns the admin's session cookie.
    pub async fn setup_admin(&self) -> String {
        let token = np_api::issue_setup_token(&self.store).await.unwrap();
        let res = self
            .post(
                "/api/v1/setup",
                serde_json::json!({
                    "token": token, "instance_name": "Acme", "name": "Ana",
                    "email": "ana@example.com", "password": "correct horse"
                }),
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "{}", res.body);
        res.cookie()
    }
}

impl TestApp {
    pub async fn patch(&self, path: &str, body: Value, cookie: Option<&str>) -> Res {
        self.method("PATCH", path, body, cookie).await
    }

    pub async fn delete(&self, path: &str, cookie: Option<&str>) -> Res {
        self.method("DELETE", path, Value::Null, cookie).await
    }

    pub async fn method(&self, method: &str, path: &str, body: Value, cookie: Option<&str>) -> Res {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::HOST, "localhost:12313")
            .header(header::ORIGIN, "http://localhost:12313");
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let body = if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        };
        self.send(b.body(body).unwrap()).await
    }

    /// Creates a user directly in the store and logs in; returns the session cookie.
    pub async fn user(&self, email: &str) -> String {
        let hash = np_api::auth::hash_password("password123".into())
            .await
            .unwrap();
        self.store
            .create_user(
                email,
                &email[..email.find('@').unwrap()],
                Some(&hash),
                false,
            )
            .await
            .unwrap();
        self.login(email, "password123").await
    }

    pub async fn login(&self, email: &str, password: &str) -> String {
        let r = self
            .post(
                "/api/v1/auth/login",
                serde_json::json!({ "email": email, "password": password }),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "login {email}: {}", r.body);
        r.cookie()
    }

    /// Creates workspace `slug` owned by `owner`.
    pub async fn workspace(&self, owner: &str, slug: &str) {
        let r = self
            .post(
                "/api/v1/workspaces",
                serde_json::json!({ "name": slug.to_uppercase(), "slug": slug }),
                Some(owner),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    }

    /// Invites `email` with `role` and accepts as that (new) user; returns their cookie.
    pub async fn join(&self, inviter: &str, slug: &str, email: &str, role: &str) -> String {
        let r = self
            .post(
                &format!("/api/v1/workspaces/{slug}/invites"),
                serde_json::json!({ "invites": [{ "email": email, "role": role }] }),
                Some(inviter),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        let link = r.body[0]["link"]
            .as_str()
            .unwrap_or_else(|| panic!("{}", r.body))
            .to_string();
        let token = link.rsplit('/').next().unwrap();
        let r = self
            .post(
                &format!("/api/v1/invites/{token}/signup"),
                serde_json::json!({ "name": "New", "password": "password123" }),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.cookie()
    }

    pub async fn user_id(&self, cookie: &str) -> String {
        self.get("/api/v1/me", Some(cookie)).await.body["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

impl TestApp {
    /// POSTs one file as multipart field `file`.
    pub async fn upload(&self, path: &str, name: &str, bytes: &[u8], cookie: &str) -> Res {
        let boundary = "XnpBoundary";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::post(path)
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header(header::HOST, "localhost:12313")
            .header(header::ORIGIN, "http://localhost:12313")
            .header(header::COOKIE, cookie)
            .body(Body::from(body))
            .unwrap();
        self.send(req).await
    }

    /// GETs raw bytes and headers.
    pub async fn raw(
        &self,
        path: &str,
        cookie: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let res = self
            .app
            .clone()
            .oneshot(
                Request::get(path)
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, bytes)
    }
}
