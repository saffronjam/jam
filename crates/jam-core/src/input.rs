//! Persistent input channel: sending a key never waits for a network round trip.
use crate::*;
use anyhow::{Result, bail};
use std::{
    collections::HashMap,
    io,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread,
};
struct Link {
    stream: UnixStream,
    error: Arc<Mutex<Option<String>>>,
}
#[derive(Default)]
struct Connection {
    incarnation: Option<String>,
    link: Option<Link>,
}
type Slot = Arc<Mutex<Connection>>;
static LINKS: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();
fn connection(path: &Path) -> Slot {
    LINKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .entry(path.to_owned())
        .or_default()
        .clone()
}
/// Discard an input stream to a replaced server before accepting fresh input.
/// No queued or already transmitted request is copied to the new connection.
pub fn observe_server(path: &Path, incarnation: &str) {
    let slot = connection(path);
    let mut slot = slot.lock().unwrap();
    if slot.incarnation.as_deref() != Some(incarnation) {
        slot.link = None;
        slot.incarnation = Some(incarnation.to_owned());
    }
}
pub fn send(path: &Path, request: Request) -> Result<Message> {
    let slot = connection(path);
    let mut slot = slot.lock().unwrap();
    if let Some(link) = slot.link.as_ref() {
        let error = link.error.lock().unwrap().take();
        if let Some(error) = error {
            slot.link = None;
            bail!(error);
        }
    }
    if slot.link.is_none() {
        let mut stream = connect_to(path)?;
        write_frame(&mut stream, &Request::InputStream)?;
        match read_frame::<Message>(&mut stream)? {
            Message::Ok => (),
            Message::Error(e) => bail!(e),
            _ => bail!("Invalid input channel handshake"),
        }
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        stream.set_read_timeout(None)?;
        let mut reader = stream.try_clone()?;
        let error = Arc::new(Mutex::new(None));
        let errors = error.clone();
        thread::spawn(move || {
            loop {
                match read_frame::<Message>(&mut reader) {
                    Ok(Message::Ok) => (),
                    Ok(Message::Error(e)) => {
                        *errors.lock().unwrap() = Some(e);
                    }
                    Ok(_) => {
                        *errors.lock().unwrap() = Some("Invalid input acknowledgement".into());
                        break;
                    }
                    Err(e) => {
                        *errors.lock().unwrap() = Some(format!(
                            "Terminal input connection closed: {e}. Input was not replayed."
                        ));
                        break;
                    }
                }
            }
        });
        slot.link = Some(Link { stream, error });
    }
    if let Err(error) = write_request(&mut slot.link.as_mut().unwrap().stream, &request) {
        slot.link = None;
        return Err(io::Error::other(error.to_string()).into());
    }
    Ok(Message::Ok)
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_replacement_resets_input_without_replaying_previous_requests() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("input.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let accept = || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                write_frame(&mut stream, &Message::Hello(VERSION.into())).unwrap();
                assert!(matches!(
                    read_frame::<Request>(&mut stream).unwrap(),
                    Request::InputStream
                ));
                write_frame(&mut stream, &Message::Ok).unwrap();
                stream
            };
            let read = |stream: &mut UnixStream, expected: &str| {
                let Request::Client { request, .. } = read_frame::<Request>(stream).unwrap() else {
                    panic!("missing client envelope");
                };
                let Request::Send { data, .. } = *request else {
                    panic!("unexpected request")
                };
                assert_eq!(data, expected.as_bytes());
            };
            let mut old = accept();
            read(&mut old, "first");
            read(&mut old, "second");
            // Observing the same incarnation must keep the original link;
            // observing the next one closes it before fresh input is sent.
            assert!(read_frame::<Request>(&mut old).is_err());
            let mut new = accept();
            read(&mut new, "fresh");
            write_frame(&mut new, &Message::Ok).unwrap();
        });
        observe_server(&path, "old");
        for text in ["first", "second"] {
            observe_server(&path, "old");
            send(
                &path,
                Request::Send {
                    pane: "pane".into(),
                    data: text.as_bytes().to_vec(),
                },
            )
            .unwrap();
        }
        observe_server(&path, "new");
        send(
            &path,
            Request::Send {
                pane: "pane".into(),
                data: b"fresh".to_vec(),
            },
        )
        .unwrap();
        server.join().unwrap();
    }
    #[test]
    fn inputs_pipeline_without_waiting_for_acknowledgements() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("input.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            write_frame(&mut stream, &Message::Hello(VERSION.into())).unwrap();
            assert!(matches!(
                read_frame::<Request>(&mut stream).unwrap(),
                Request::InputStream
            ));
            write_frame(&mut stream, &Message::Ok).unwrap();
            // The second input must arrive before we acknowledge the first.
            for _ in 0..2 {
                assert!(matches!(
                    read_frame::<Request>(&mut stream).unwrap(),
                    Request::Client { .. }
                ));
            }
            write_frame(&mut stream, &Message::Ok).unwrap();
            write_frame(&mut stream, &Message::Ok).unwrap();
        });
        for text in ["first", "second"] {
            send(
                &path,
                Request::Send {
                    pane: "pane".into(),
                    data: text.as_bytes().to_vec(),
                },
            )
            .unwrap();
        }
        server.join().unwrap();
    }
}
