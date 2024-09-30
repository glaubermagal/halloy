use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::{format::SecondsFormat, DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::history::{dir_path, Error, Kind};
use crate::{message, server, Message};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct Metadata {
    pub read_marker: Option<ReadMarker>,
    pub last_triggers_unread: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize, Serialize)]
pub struct ReadMarker(DateTime<Utc>);

impl ReadMarker {
    pub fn latest(messages: &[Message]) -> Option<Self> {
        messages
            .iter()
            .rev()
            .find(|message| !matches!(message.target.source(), message::Source::Internal(_)))
            .map(|message| message.server_time)
            .map(Self)
    }

    pub fn date_time(self) -> DateTime<Utc> {
        self.0
    }
}

impl FromStr for ReadMarker {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Self)
    }
}

impl fmt::Display for ReadMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.to_rfc3339_opts(SecondsFormat::Millis, true).fmt(f)
    }
}

pub fn latest_triggers_unread(messages: &[Message]) -> Option<DateTime<Utc>> {
    messages
        .iter()
        .rev()
        .find(|message| message.triggers_unread())
        .map(|message| message.server_time)
}

pub async fn load(server: server::Server, kind: Kind) -> Result<Metadata, Error> {
    let path = path(&server, &kind).await?;

    if let Ok(bytes) = fs::read(path).await {
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    } else {
        Ok(Metadata::default())
    }
}

pub async fn save(
    server: &server::Server,
    kind: &Kind,
    messages: &[Message],
    read_marker: Option<ReadMarker>,
) -> Result<(), Error> {
    let bytes = serde_json::to_vec(&Metadata {
        read_marker,
        last_triggers_unread: latest_triggers_unread(messages),
    })?;

    let path = path(server, kind).await?;

    fs::write(path, &bytes).await?;

    Ok(())
}

pub async fn update(
    server: &server::Server,
    kind: &Kind,
    read_marker: &ReadMarker,
) -> Result<(), Error> {
    let metadata = load(server.clone(), kind.clone()).await?;

    if metadata
        .read_marker
        .is_some_and(|metadata_read_marker| metadata_read_marker >= *read_marker)
    {
        return Ok(());
    }

    let bytes = serde_json::to_vec(&Metadata {
        read_marker: Some(*read_marker),
        last_triggers_unread: metadata.last_triggers_unread,
    })?;

    let path = path(server, kind).await?;

    fs::write(path, &bytes).await?;

    Ok(())
}

async fn path(server: &server::Server, kind: &Kind) -> Result<PathBuf, Error> {
    let dir = dir_path().await?;

    let name = match kind {
        Kind::Server => format!("{server}-metadata"),
        Kind::Channel(channel) => format!("{server}channel{channel}-metadata"),
        Kind::Query(nick) => format!("{server}nickname{}-metadata", nick),
        Kind::Logs => "log-metadata".to_string(),
    };

    let hashed_name = seahash::hash(name.as_bytes());

    Ok(dir.join(format!("{hashed_name}.json")))
}
