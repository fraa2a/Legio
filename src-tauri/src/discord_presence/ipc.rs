use serde_json::{Value, json};
use std::{io, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::timeout,
};

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FRAME_BYTES: usize = 64 * 1024;

#[cfg(target_os = "linux")]
type Stream = tokio::net::UnixStream;
#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

pub(super) struct Connection {
    stream: Stream,
}

impl Connection {
    pub(super) async fn connect(application_id: &str) -> io::Result<Self> {
        timeout(IO_TIMEOUT, async {
            let mut connection = Self {
                stream: connect_stream().await?,
            };
            handshake(&mut connection.stream, application_id).await?;
            Ok(connection)
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Discord handshake timed out"))?
    }

    pub(super) async fn set_activity(&mut self, activity: Option<Value>) -> io::Result<()> {
        timeout(IO_TIMEOUT, exchange_activity(&mut self.stream, activity))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "Discord activity update timed out")
            })?
    }
}

async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    application_id: &str,
) -> io::Result<()> {
    write_frame(stream, 0, &json!({ "v": 1, "client_id": application_id })).await?;
    let (opcode, response) = read_frame(stream).await?;
    if opcode != 1 || response["evt"] != "READY" {
        return Err(io::Error::other("Discord rejected the IPC handshake"));
    }
    Ok(())
}

async fn exchange_activity<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    activity: Option<Value>,
) -> io::Result<()> {
    let nonce = uuid::Uuid::new_v4().to_string();
    write_frame(stream, 1, &json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": activity }, "nonce": nonce })).await?;
    loop {
        let (opcode, response) = read_frame(stream).await?;
        match opcode {
            2 => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Discord closed the IPC connection",
                ));
            }
            3 => write_frame(stream, 4, &response).await?,
            1 if response["nonce"] == nonce => {
                if response["evt"] == "ERROR" || response["cmd"] != "SET_ACTIVITY" {
                    return Err(io::Error::other("Discord rejected the activity update"));
                }
                return Ok(());
            }
            1 | 4 => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Discord IPC opcode",
                ));
            }
        }
    }
}

async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    opcode: u32,
    payload: &Value,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(payload)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Discord IPC frame is too large",
        ));
    }
    stream.write_all(&opcode.to_le_bytes()).await?;
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<(u32, Value)> {
    let opcode = stream.read_u32_le().await?;
    let length = stream.read_u32_le().await? as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Discord IPC frame is too large",
        ));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok((opcode, serde_json::from_slice(&bytes)?))
}

#[cfg(target_os = "linux")]
async fn connect_stream() -> io::Result<Stream> {
    let roots = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .chain(std::iter::once(std::path::PathBuf::from("/tmp")));
    for root in roots {
        if !root.is_absolute() {
            continue;
        }
        for subdirectory in [
            "",
            "app/com.discordapp.Discord",
            "app/dev.vencord.Vesktop",
            ".flatpak/com.discordapp.Discord/xdg-run",
            ".flatpak/dev.vencord.Vesktop/xdg-run",
            "snap.discord",
            "snap.discord-canary",
        ] {
            for index in 0..10 {
                let path = root.join(subdirectory).join(format!("discord-ipc-{index}"));
                if let Ok(stream) = Stream::connect(path).await {
                    return Ok(stream);
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord desktop IPC is not available",
    ))
}

#[cfg(windows)]
async fn connect_stream() -> io::Result<Stream> {
    for index in 0..10 {
        if let Ok(stream) = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(format!(r"\\.\pipe\discord-ipc-{index}"))
        {
            return Ok(stream);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord desktop IPC is not available",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handshake_requires_ready_and_activity_detects_disconnects() {
        for ready in [true, false] {
            let (mut client, mut server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(async move {
                let (opcode, request) = read_frame(&mut server).await.unwrap();
                assert_eq!(opcode, 0);
                assert_eq!(request["client_id"], "123456789012345678");
                write_frame(
                    &mut server,
                    1,
                    &json!({"evt": if ready { "READY" } else { "ERROR" }}),
                )
                .await
                .unwrap();
            });
            assert_eq!(
                handshake(&mut client, "123456789012345678").await.is_ok(),
                ready
            );
            peer.await.unwrap();
        }
        let (mut client, server) = tokio::io::duplex(4096);
        drop(server);
        assert!(exchange_activity(&mut client, None).await.is_err());
    }

    #[tokio::test]
    async fn acknowledges_updates_handles_ping_and_clears_presence() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let peer = tokio::spawn(async move {
            for expected in [Some(json!({"details": "A game"})), None] {
                let (opcode, request) = read_frame(&mut server).await.unwrap();
                assert_eq!(opcode, 1);
                assert_eq!(request["args"]["activity"], json!(expected));
                write_frame(&mut server, 3, &json!({"ping": true}))
                    .await
                    .unwrap();
                assert_eq!(read_frame(&mut server).await.unwrap().0, 4);
                write_frame(
                    &mut server,
                    1,
                    &json!({"cmd": "SET_ACTIVITY", "nonce": request["nonce"]}),
                )
                .await
                .unwrap();
            }
        });
        exchange_activity(&mut client, Some(json!({"details": "A game"})))
            .await
            .unwrap();
        exchange_activity(&mut client, None).await.unwrap();
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_frames_before_allocating_and_rejected_activities() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        server.write_u32_le(1).await.unwrap();
        server
            .write_u32_le((MAX_FRAME_BYTES + 1) as u32)
            .await
            .unwrap();
        assert_eq!(
            read_frame(&mut client).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let peer = tokio::spawn(async move {
            let (_, request) = read_frame(&mut server).await.unwrap();
            write_frame(
                &mut server,
                1,
                &json!({"cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": request["nonce"]}),
            )
            .await
            .unwrap();
        });
        assert!(exchange_activity(&mut client, None).await.is_err());
        peer.await.unwrap();
    }
}
