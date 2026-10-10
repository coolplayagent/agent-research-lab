//! Bounded real TCP/SSE benchmark. No model execution or synthetic latency ticks.
use super::transport::Web;
use super::*;
use std::sync::{Mutex, atomic::AtomicUsize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{Semaphore, watch},
    task::JoinSet,
};

pub(super) async fn request(
    addr: SocketAddr,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<Value> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;
    let body = body
        .map(serde_json::to_vec)
        .transpose()?
        .unwrap_or_default();
    let method = if body.is_empty() { "GET" } else { "POST" };
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let header = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    let mut raw = Vec::new();
    stream.take(1024 * 1024).read_to_end(&mut raw).await?;
    let at = raw
        .windows(4)
        .position(|s| s == b"\r\n\r\n")
        .context("incomplete HTTP response")?;
    ensure!(
        raw.starts_with(b"HTTP/1.1 200"),
        "request rejected: {}",
        String::from_utf8_lossy(&raw[at + 4..])
    );
    Ok(serde_json::from_slice(&raw[at + 4..])?)
}
pub(super) struct Events {
    reader: BufReader<TcpStream>,
    pending: Vec<u8>,
}
impl Events {
    pub(super) async fn connect(addr: SocketAddr, path: &str, token: Option<&str>) -> Result<Self> {
        Self::open(addr, path, token, None).await
    }
    pub(super) async fn open(
        addr: SocketAddr,
        path: &str,
        token: Option<&str>,
        cookie: Option<&str>,
    ) -> Result<Self> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let cookie = cookie
            .map(|c| format!("Cookie: crystal_operator={c}\r\n"))
            .unwrap_or_default();
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}{cookie}\r\n").as_bytes(),
            )
            .await?;
        let mut reader = BufReader::new(stream);
        let mut header = String::new();
        reader.read_line(&mut header).await?;
        ensure!(
            header.starts_with("HTTP/1.1 200"),
            "stream rejected: {}",
            header.trim()
        );
        loop {
            let mut line = String::new();
            ensure!(
                reader.read_line(&mut line).await? > 0,
                "stream closed in headers"
            );
            header.push_str(&line);
            ensure!(header.len() < 8192, "large response headers");
            if line == "\r\n" {
                break;
            }
        }
        ensure!(
            header
                .to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "expected HTTP/1.1 chunked SSE"
        );
        Ok(Self {
            reader,
            pending: Vec::new(),
        })
    }
    pub(super) async fn next(&mut self) -> Result<(String, Value)> {
        loop {
            if let Some(end) = self.pending.windows(2).position(|w| w == b"\n\n") {
                let raw = self.pending.drain(..end + 2).collect::<Vec<_>>();
                let text = std::str::from_utf8(&raw)?;
                let kind = text
                    .lines()
                    .find_map(|l| l.strip_prefix("event: "))
                    .unwrap_or("message");
                if let Some(data) = text.lines().find_map(|l| l.strip_prefix("data: ")) {
                    return Ok((kind.into(), serde_json::from_str(data)?));
                }
                continue;
            }
            let mut line = String::new();
            ensure!(self.reader.read_line(&mut line).await? > 0, "stream closed");
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16)?;
            ensure!(
                size > 0 && size <= 1024 * 1024,
                "stream ended or oversized chunk"
            );
            let mut chunk = vec![0u8; size + 2];
            self.reader.read_exact(&mut chunk).await?;
            ensure!(&chunk[size..] == b"\r\n", "invalid chunk terminator");
            self.pending.extend_from_slice(&chunk[..size]);
            ensure!(self.pending.len() <= 1024 * 1024, "oversized SSE event");
        }
    }
}
fn distribution(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    let at = |q: f64| {
        values
            .get(((values.len().saturating_sub(1)) as f64 * q).ceil() as usize)
            .copied()
    };
    json!({"samples":values.len(),"p50":at(0.5),"p95":at(0.95),"p99":at(0.99),"max":values.last()})
}
fn fd_capacity() -> Result<u64> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid pointer; only this bounded benchmark process changes its soft limit.
    ensure!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } == 0,
        "cannot read descriptor limit"
    );
    limit.rlim_cur = limit.rlim_max.min(65536);
    ensure!(
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) } == 0,
        "cannot set benchmark descriptor limit"
    );
    Ok(limit.rlim_cur)
}
pub(crate) fn run(
    state: &Path,
    mode: &str,
    people: usize,
    messages: usize,
    rate: usize,
) -> Result<Value> {
    ensure!(matches!(mode, "mixed" | "hot"), "mode must be mixed or hot");
    ensure!(
        (2..=20_000).contains(&people)
            && (1..=100_000).contains(&messages)
            && (1..=10_000).contains(&rate),
        "benchmark exceeds bounds"
    );
    ensure!(
        mode == "mixed" || people.saturating_mul(messages) <= 2_000_000,
        "delivery sample budget exceeds two million"
    );
    let descriptors = fd_capacity()?;
    ensure!(
        descriptors >= people as u64 * 2 + 512,
        "insufficient descriptors"
    );
    fs::create_dir(state).context("benchmark state must be a new private directory")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(600),
            exercise(state, mode, people, messages, rate, descriptors),
        )
        .await?
    })
}
async fn exercise(
    state: &Path,
    mode: &str,
    people: usize,
    messages: usize,
    rate: usize,
    descriptors: u64,
) -> Result<Value> {
    let started = Instant::now();
    let hub = crystal::Hub::open(state)?;
    let actors = (0..people)
        .map(|i| format!("bench-person-{i:05}"))
        .collect::<Vec<_>>();
    hub.register(actors.clone()).await?;
    let groups = if mode == "mixed" { people } else { 1 };
    for i in 0..groups {
        hub.create_group(crystal::NewGroup {
            id: format!("bench-group-{i:05}"),
            title: format!("Research group {i}"),
            topic: "Performance experiment with real network and durable logs".into(),
            private: false,
            kind: Default::default(),
            members: if mode == "mixed" {
                vec![actors[i].clone(), actors[(i + 1) % people].clone()]
            } else {
                actors.clone()
            },
        })
        .await?;
    }
    let mut tokens = Vec::new();
    for actor in &actors {
        tokens.push(hub.grant(actor, 3600).await?);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (updates, _) = watch::channel(0);
    let (shutdown, _) = watch::channel(false);
    let web = Web::new(
        hub.clone(),
        addr,
        None,
        Arc::new(RwLock::new(json!({}))),
        updates,
        shutdown.clone(),
    );
    let mut stop = shutdown.subscribe();
    let server = tokio::spawn(async move {
        axum::serve(listener, transport::router(web))
            .with_graceful_shutdown(async move {
                let _ = stop.changed().await;
            })
            .await
    });
    let prepare_ms = started.elapsed().as_secs_f64() * 1000.0;
    let starts = Arc::new(Mutex::new(vec![None::<Instant>; messages]));
    let delivered = Arc::new(AtomicUsize::new(0));
    let deliveries = Arc::new(Mutex::new(Vec::<f64>::new()));
    let ready = Arc::new(AtomicUsize::new(0));
    let mut clients = JoinSet::new();
    let admission = Arc::new(Semaphore::new(128));
    for (i, token) in tokens.iter().enumerate() {
        let token = token.clone();
        let starts = starts.clone();
        let delivered = delivered.clone();
        let deliveries = deliveries.clone();
        let ready = ready.clone();
        let admission = admission.clone();
        let group = if mode == "mixed" { i } else { 0 };
        clients.spawn(async move {
            let permit = admission.acquire_owned().await?;
            let mut connection = Events::connect(
                addr,
                &format!("/api/crystal/stream?group_id=bench-group-{group:05}&after=0"),
                Some(&token),
            )
            .await?;
            ensure!(
                connection.next().await?.0 == "ready",
                "missing stream readiness"
            );
            drop(permit);
            ready.fetch_add(1, Ordering::Release);
            let mut seen = BTreeSet::new();
            loop {
                let (kind, value) = connection.next().await?;
                if kind != "message" {
                    continue;
                }
                let n: usize = value["request_id"]
                    .as_str()
                    .context("missing request identity")?
                    .strip_prefix("load-")
                    .context("invalid request identity")?
                    .parse()?;
                ensure!(seen.insert(n), "duplicate stream delivery");
                let start = starts.lock().unwrap()[n].context("delivery precedes publisher")?;
                deliveries
                    .lock()
                    .unwrap()
                    .push(start.elapsed().as_secs_f64() * 1000.0);
                delivered.fetch_add(1, Ordering::Release);
            }
            #[allow(unreachable_code)]
            Ok::<_, anyhow::Error>(())
        });
    }
    while ready.load(Ordering::Acquire) < people {
        tokio::select! {result=clients.join_next()=>{result.context("clients disappeared")??.context("client failed during setup")?;},_=tokio::time::sleep(Duration::from_millis(10))=>{}}
    }
    let connected_ms = started.elapsed().as_secs_f64() * 1000.0 - prepare_ms;
    let load_start = Instant::now();
    let mut sends = JoinSet::new();
    let publishing = Arc::new(Semaphore::new(64));
    let mut acknowledgements = Vec::with_capacity(messages);
    for n in 0..messages {
        let scheduled = load_start + Duration::from_secs_f64(n as f64 / rate as f64);
        tokio::time::sleep_until(scheduled.into()).await;
        let group = if mode == "mixed" { n % people } else { 0 };
        let actor = if mode == "mixed" {
            (group + 1) % people
        } else {
            n % people
        };
        let token = tokens[actor].clone();
        let starts = starts.clone();
        let permit = publishing.clone().acquire_owned().await?;
        sends.spawn(async move {let _permit=permit;let start=Instant::now();starts.lock().unwrap()[n]=Some(start);
            let value=request(addr,"/api/crystal/send",Some(&token),Some(&json!({"group_id":format!("bench-group-{group:05}"),"request_id":format!("load-{n}"),"text":"x".repeat(256)}))).await?;
            ensure!(value["durable"]==true && value["duplicate"]==false,"invalid storage receipt");Ok::<_,anyhow::Error>(start.elapsed().as_secs_f64()*1000.0)});
    }
    while let Some(result) = sends.join_next().await {
        acknowledgements.push(result??);
    }
    let expected = if mode == "mixed" {
        messages
    } else {
        messages * people
    };
    while delivered.load(Ordering::Acquire) < expected {
        tokio::select! {result=clients.join_next()=>{result.context("clients disappeared")??.context("client failed during load")?;},_=tokio::time::sleep(Duration::from_millis(10))=>{}}
    }
    let elapsed = load_start.elapsed().as_secs_f64();
    let metrics = hub.metrics();
    let delivery_ms = distribution(std::mem::take(&mut *deliveries.lock().unwrap()));
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    shutdown.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    Ok(
        json!({"schema_version":1,"runtime_workers":4,"mode":mode,"people":people,"groups":groups,"live_connections":people,"messages":messages,"deliveries":expected,"payload_bytes":256,"offered_messages_per_second":rate,"achieved_messages_per_second":messages as f64/elapsed,"load_seconds":elapsed,"prepare_ms":prepare_ms,"connect_ms":connected_ms,"publish_ack_ms":distribution(acknowledgements),"recipient_delivery_ms":delivery_ms,"ui_visible_ms":null,"descriptor_limit":descriptors,"transport":"real loopback TCP HTTP/1.1 SSE","storage":"real SQLite WAL synchronous=FULL","metrics":metrics,"limits":"Single host, no model inference, no cross-host replication; UI measured separately"}),
    )
}
