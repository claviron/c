//! Real Link message-profile check against a running orbit-echo process.
//! This does not create a public QUIC/WebTransport connection.
use claviron::orbit::link::application::{
    ApplicationClient, Exchange, ExchangeBody, HttpVersion, Message,
    MessageReader, write_message,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn http(
    client: &ApplicationClient,
    application: &str,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Exchange {
        method: "POST".into(),
        path: "/echo".into(),
        raw_path: b"/echo".to_vec(),
        body: if body.is_empty() {
            ExchangeBody::Empty
        } else {
            ExchangeBody::Stream
        },
        ..Default::default()
    };
    let (mut input, mut output, reply, _) =
        client.send(application, request)?.split();
    let send = async {
        if !body.is_empty() {
            input.write_all(body).await?;
            input.shutdown().await?;
        }
        Ok::<_, std::io::Error>(())
    };
    let receive = async {
        assert_eq!(reply.head().await?.status, 200);
        let mut actual = Vec::new();
        output.read_to_end(&mut actual).await?;
        assert_eq!(actual, body);
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (sent, received) = tokio::join!(send, receive);
    sent?;
    received
}

async fn route(
    client: &ApplicationClient,
    application: &str,
    path: &str,
    status: u16,
    expected: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Exchange {
        method: "GET".into(),
        path: path.into(),
        raw_path: path.as_bytes().to_vec(),
        ..Default::default()
    };
    let (_input, mut output, reply, _) =
        client.send(application, request)?.split();
    assert_eq!(reply.head().await?.status, status);
    let mut actual = Vec::new();
    output.read_to_end(&mut actual).await?;
    assert_eq!(actual, expected);
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let fleet = args.next().ok_or("expected FLEET APPLICATION")?;
    let application = args.next().ok_or("expected FLEET APPLICATION")?;
    let session_path = args.next().unwrap_or_else(|| "/echo".into());
    let websocket_only =
        match args.next().as_deref() {
            None => false,
            Some("--websocket-only") => true,
            _ => return Err(
                "expected FLEET APPLICATION [SESSION_PATH] [--websocket-only]"
                    .into(),
            ),
        };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let client = ApplicationClient::join(&fleet, "c-link-check", 1)?;
    // The optional session path selects the application examples' shared routes;
    // the binding's minimal echo executable only provides /echo.
    if session_path == "/ws" {
        route(&client, &application, "/bench/plain", 200, b"ok\n").await?;
        route(
            &client,
            &application,
            "/bench/json",
            200,
            b"{\"message\":\"ok\"}",
        )
        .await?;
        route(&client, &application, "/missing", 404, b"not found\n").await?;
        println!("Link: plain, JSON and missing routes passed");
    }
    http(&client, &application, &[]).await?;
    let inline = Exchange {
        method: "POST".into(),
        path: "/echo".into(),
        raw_path: b"/echo".to_vec(),
        body: ExchangeBody::Inline(tokio_util::bytes::Bytes::from_static(
            b"inline body",
        )),
        ..Default::default()
    };
    let (_input, mut output, reply, _) =
        client.send(&application, inline)?.split();
    assert_eq!(reply.head().await?.status, 200);
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes).await?;
    assert_eq!(bytes, b"inline body");
    let payload = vec![0xa5; 65536];
    for result in futures_util::future::join_all(
        (0..8).map(|_| http(&client, &application, &payload)),
    )
    .await
    {
        result?;
    }
    println!(
        "Link: HTTP empty body and eight concurrent 64 KiB streams passed"
    );
    for version in [HttpVersion::Http11, HttpVersion::Http2, HttpVersion::Http3]
    {
        if websocket_only && version == HttpVersion::Http3 {
            continue;
        }
        let request = Exchange {
            method: "GET".into(),
            scheme: "https".into(),
            path: session_path.clone(),
            raw_path: session_path.as_bytes().to_vec(),
            upgrade: true,
            body: ExchangeBody::Stream,
            version,
            ..Default::default()
        };
        let (mut input, mut output, reply, _) =
            client.send(&application, request)?.split();
        assert_eq!(reply.head().await?.status, 101);
        let mut messages = MessageReader::new();
        for message in [
            Message::Text("hello from Link".into()),
            Message::Binary(vec![0, 255, 7, 128]),
            Message::Close { code: 4001, reason: "done".into() },
        ] {
            write_message(&mut input, &message).await?;
            assert_eq!(messages.read(&mut output).await?, Some(message));
        }
        assert_eq!(messages.read(&mut output).await?, None);
    }
    println!(
        "Link: {} reliable-message profiles passed (no public QUIC handshake)",
        if websocket_only { "H1/H2; WT not requested" } else { "H1/H2/WT" }
    );
    client.leave();
    Ok(())
}
