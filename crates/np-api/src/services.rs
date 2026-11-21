//! Outside services an instance admin can switch on (S3.7, as in Plane): an AI writing
//! assistant on any OpenAI-compatible Chat Completions API, and Unsplash photos for project
//! covers. Both are off until a key is saved; nothing is sent anywhere before that. Keys live in
//! `instance_config` and never go back to a browser.

use np_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const DEFAULT_AI_BASE: &str = "https://api.openai.com/v1";
pub const DEFAULT_AI_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, Default)]
pub struct Ai {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Ai {
    pub fn enabled(&self) -> bool {
        !self.api_key.is_empty()
    }
}

pub async fn ai(store: &Store) -> np_core::Result<Ai> {
    let c = store.config("ai.").await?;
    let get = |k: &str| c.get(k).cloned().unwrap_or_default();
    let model = get("ai.model");
    let base = get("ai.base_url");
    Ok(Ai {
        api_key: get("ai.api_key"),
        model: if model.is_empty() {
            DEFAULT_AI_MODEL.into()
        } else {
            model
        },
        base_url: if base.is_empty() {
            DEFAULT_AI_BASE.into()
        } else {
            base
        },
    })
}

pub async fn unsplash_key(store: &Store) -> np_core::Result<String> {
    Ok(store
        .config("unsplash.")
        .await?
        .get("unsplash.access_key")
        .cloned()
        .unwrap_or_default())
}

/// What the assistant is asked: the task first, then the text it works on (as in Plane).
pub async fn complete(
    http: &reqwest::Client,
    ai: &Ai,
    task: &str,
    prompt: &str,
) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Msg {
        content: Option<String>,
    }
    #[derive(Deserialize)]
    struct Choice {
        message: Msg,
    }
    #[derive(Deserialize)]
    struct Answer {
        choices: Vec<Choice>,
    }
    let res = http
        .post(format!("{}/chat/completions", ai.base_url.trim_end_matches('/')))
        .bearer_auth(&ai.api_key)
        .timeout(std::time::Duration::from_secs(60))
        .json(&json!({
            "model": ai.model,
            "messages": [
                { "role": "system", "content": "You help people write work items in a project management tool. Answer with the text only, in the language of the request, using short paragraphs or lists." },
                { "role": "user", "content": format!("{task}\n{prompt}") },
            ],
        }))
        .send()
        .await?;
    let status = res.status();
    if !status.is_success() {
        anyhow::bail!("the AI service answered {status}");
    }
    let a: Answer = res.json().await?;
    Ok(a.choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// One Unsplash photo as the cover picker needs it, with its credit.
#[derive(Debug, Clone, Serialize)]
pub struct Photo {
    pub id: String,
    pub thumb: String,
    pub regular: String,
    pub description: String,
    pub author: String,
    pub author_url: String,
    /// Unsplash asks to be told when a photo is used (its download endpoint).
    pub download_location: String,
}

/// Unsplash's latest photos, or a search, 20 at a time.
pub async fn unsplash_photos(
    http: &reqwest::Client,
    base: &str,
    key: &str,
    query: Option<&str>,
    page: u32,
) -> anyhow::Result<Vec<Photo>> {
    #[derive(Deserialize)]
    struct Urls {
        small: String,
        regular: String,
    }
    #[derive(Deserialize)]
    struct UserLinks {
        html: String,
    }
    #[derive(Deserialize)]
    struct User {
        name: String,
        links: UserLinks,
    }
    #[derive(Deserialize)]
    struct Links {
        download_location: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        id: String,
        alt_description: Option<String>,
        urls: Urls,
        user: User,
        links: Links,
    }
    #[derive(Deserialize)]
    struct Search {
        results: Vec<Raw>,
    }
    let q = query.map(str::trim).filter(|q| !q.is_empty());
    let mut url = reqwest::Url::parse(&match q {
        Some(_) => format!("{base}/search/photos"),
        None => format!("{base}/photos"),
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(q) = q {
            pairs
                .append_pair("query", q)
                .append_pair("orientation", "landscape");
        }
        pairs
            .append_pair("page", &page.to_string())
            .append_pair("per_page", "20");
    }
    let req = http.get(url);
    let res = req
        .header("Authorization", format!("Client-ID {key}"))
        .header("Accept-Version", "v1")
        .send()
        .await?
        .error_for_status()?;
    let raw: Vec<Raw> = if q.is_some() {
        res.json::<Search>().await?.results
    } else {
        res.json().await?
    };
    Ok(raw
        .into_iter()
        .map(|r| Photo {
            id: r.id,
            thumb: r.urls.small,
            regular: r.urls.regular,
            description: r.alt_description.unwrap_or_default(),
            author: r.user.name,
            author_url: r.user.links.html,
            download_location: r.links.download_location,
        })
        .collect())
}

/// Tells Unsplash a photo was chosen (required by its API guidelines). Failures only log.
pub async fn unsplash_track(
    http: &reqwest::Client,
    base: &str,
    key: &str,
    download_location: &str,
) {
    let ok = download_location.starts_with(&format!("{base}/photos/"));
    if !ok {
        tracing::warn!("ignored an Unsplash download link that is not Unsplash's");
        return;
    }
    if let Err(e) = http
        .get(download_location)
        .header("Authorization", format!("Client-ID {key}"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
    {
        tracing::warn!(error = %e, "could not tell Unsplash a photo was used");
    }
}
