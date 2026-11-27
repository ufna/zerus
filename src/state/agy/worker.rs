use super::{
    acp::{Client, Reply},
    *,
};

const SEND_PROOF: Duration = Duration::from_secs(4);
const TURN_LIMIT: Duration = Duration::from_secs(60 * 60);
const EVENT_COUNT: usize = 32;
const EVENT_BYTES: usize = 2048;

struct Turn {
    id: u64,
    receipt: Value,
    confirm_by: Instant,
    ends_by: Instant,
    client: Option<UnixStream>,
    interruption: Option<UnixStream>,
    text: String,
}

struct Owner {
    native: Client,
    record: Value,
    turn: Option<Turn>,
}

fn disposition(status: &str, error: &str) -> Value {
    json!({"status":status,"error":error})
}

fn result(reply: Reply) -> Result<Value> {
    match reply {
        Reply::Result(value) => Ok(value),
        Reply::AuthRequired => Err("Antigravity sign-in required; sign in from Terminal".into()),
        Reply::Refusal => Err("Antigravity refused the native operation".into()),
    }
}

fn edit(record: &mut Value, replace: impl FnOnce(&mut Value)) -> Result<()> {
    let _guard = state_guard()?;
    edit_locked(record, replace)
}

fn edit_locked(record: &mut Value, replace: impl FnOnce(&mut Value)) -> Result<()> {
    let current = binding(string(record, "name"))?;
    verify(&current, record)?;
    if current["agy_worker"] != record["agy_worker"] {
        return Err("Antigravity worker binding changed".into());
    }
    replace(record);
    write(record)
}

fn receipt_path(id: &str) -> Result<PathBuf> {
    if !session_id::canonical(id) {
        return Err("Invalid Antigravity request identity".into());
    }
    Ok(root().join("agy-receipts").join(format!("{id}.json")))
}

pub(super) fn receipt(id: &str, record: &Value, hash: &str) -> Result<Option<Value>> {
    let file = receipt_path(id)?;
    let Some(bytes) =
        labels::read_file(&file, 8192).map_err(|_| "Antigravity receipt is unavailable")?
    else {
        return Ok(None);
    };
    let mut answer: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Antigravity receipt is invalid")?;
    if answer["request_id"] != id
        || answer["digest"] != hash
        || ["name", "run_id", "conversation_id", "session_id"]
            .into_iter()
            .any(|key| answer[key] != record[key])
    {
        return Err("That request ID belongs to different Antigravity input".into());
    }
    if answer["status"] == "in_progress" {
        answer["status"] = json!("uncertain");
    }
    Ok(Some(answer))
}

fn save_receipt(answer: &Value) -> Result<()> {
    atomic(
        &receipt_path(string(answer, "request_id"))?,
        &answer.to_string(),
    )
}

fn clipped(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn event(record: &mut Value, kind: &str, text: &str, id: &Value) {
    let seq = record["agy_seq"].as_u64().unwrap_or(0).saturating_add(1);
    let at = now();
    record["agy_seq"] = json!(seq);
    record["last_event_at"] = json!(at);
    if !record["agy_events"].is_array() {
        record["agy_events"] = json!([]);
    }
    let mut event = json!({"seq":seq,"type":kind,"at":at,"detail":clipped(text,EVENT_BYTES),
        "request_id":id,"source":"agy_acp","run_id":record["run_id"],
        "conversation_id":record["conversation_id"]});
    if kind == "UserPromptSubmit" {
        event["delivery_state"] = json!("unconfirmed");
    }
    let events = record["agy_events"].as_array_mut().expect("event ring");
    events.push(event);
    if events.len() > EVENT_COUNT {
        events.drain(0..events.len() - EVENT_COUNT);
    }
}

impl Owner {
    fn begin(&mut self, request: &Value, mut stream: UnixStream) -> Result<()> {
        let req = string(request, "request_id");
        let hash = string(request, "digest");
        let text = string(request, "text");
        if !session_id::canonical(req)
            || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
            || !ordinary_text(text)
        {
            let _ = respond(
                &mut stream,
                &disposition("refused", "Invalid Antigravity input"),
            );
            return Ok(());
        }
        let mut answer = json!({"request_id":req,"digest":hash,"name":self.record["name"],
            "run_id":self.record["run_id"],"conversation_id":self.record["conversation_id"],
            "session_id":self.record["session_id"],"status":"in_progress","transport":"agy_acp"});
        let id = {
            let _guard = match state_guard() {
                Ok(guard) => guard,
                Err(error) => {
                    let _ = respond(&mut stream, &disposition("refused", &error));
                    return Ok(());
                }
            };
            match self.check_offer(req, hash) {
                Ok(None) => {}
                Ok(Some(previous)) => {
                    let _ = respond(&mut stream, &previous);
                    return Ok(());
                }
                Err(error) => {
                    let _ = respond(&mut stream, &disposition("refused", &error));
                    return Ok(());
                }
            }
            save_receipt(&answer)?;
            edit_locked(&mut self.record, |record| {
                record["phase"] = json!("working");
                record["activity"] = json!("busy");
                record["turn_started"] = json!(now());
            })?;
            self.native.request(
                "session/prompt",
                json!({
                    "sessionId":self.record["conversation_id"],
                    "prompt":[{"type":"text","text":text}]
                }),
            )
        };
        match id {
            Ok(id) => {
                self.turn = Some(Turn {
                    id,
                    receipt: answer,
                    client: Some(stream),
                    interruption: None,
                    confirm_by: Instant::now() + SEND_PROOF,
                    ends_by: Instant::now() + TURN_LIMIT,
                    text: String::new(),
                });
                // This is a record of the local offer. The receipt itself waits
                // for a matching ACP update or terminal response.
                edit(&mut self.record, |record| {
                    event(record, "UserPromptSubmit", text, &request["request_id"])
                })?;
                Ok(())
            }
            Err(error) => {
                answer["status"] = json!("uncertain");
                let _ = respond(&mut stream, &answer);
                Err(error)
            }
        }
    }

    // The original worker and state writer remain selected through the offer.
    // Refusals here precede both receipt reservation and provider input.
    fn check_offer(&self, request: &str, hash: &str) -> Result<Option<Value>> {
        if let Some(previous) = receipt(request, &self.record, hash)? {
            return Ok(Some(previous));
        }
        if self.turn.is_some() || self.record["phase"] != "idle" {
            return Err("Antigravity already has a turn in progress".into());
        }
        let current = binding(string(&self.record, "name"))?;
        verify(&current, &self.record)?;
        locations(&current)?;
        if current["agy_worker"] != self.record["agy_worker"] {
            return Err("Antigravity worker changed before submission".into());
        }
        Ok(None)
    }

    fn confirmed(record: &mut Value, turn: &mut Turn) -> Result<()> {
        if turn.receipt["status"] != "submitted" {
            turn.receipt["status"] = json!("submitted");
            save_receipt(&turn.receipt)?;
            if let Some(mut stream) = turn.client.take() {
                let _ = respond(&mut stream, &turn.receipt);
            }
            edit(record, |record| {
                if let Some(offer) = record["agy_events"].as_array_mut().and_then(|events| {
                    events.iter_mut().rev().find(|event| {
                        event["type"] == "UserPromptSubmit"
                            && event["request_id"] == turn.receipt["request_id"]
                    })
                }) {
                    offer["delivery_state"] = json!("submitted");
                }
            })?;
        }
        Ok(())
    }

    fn provider_frame(&mut self, frame: Value) -> Result<()> {
        let Some(turn) = self.turn.as_mut() else {
            return Ok(());
        };
        if frame["method"] == "session/update"
            && frame["params"]["sessionId"] == self.record["conversation_id"]
        {
            let update = &frame["params"]["update"];
            if update["sessionUpdate"] == "agent_message_chunk"
                && update["content"]["type"] == "text"
            {
                let chunk = string(&update["content"], "text");
                turn.text
                    .push_str(clipped(chunk, EVENT_BYTES.saturating_sub(turn.text.len())));
                Self::confirmed(&mut self.record, turn)?;
            }
        }
        let Some(reply) = Client::reply(frame, turn.id)? else {
            return Ok(());
        };
        let mut turn = self.turn.take().expect("matching Antigravity turn");
        let (settled, cancelled) = match reply {
            Reply::Result(value) => {
                let reason = string(&value, "stopReason");
                (
                    matches!(
                        reason,
                        "end_turn" | "cancelled" | "max_tokens" | "max_turn_requests" | "refusal"
                    ),
                    reason == "cancelled",
                )
            }
            Reply::Refusal | Reply::AuthRequired => (false, false),
        };
        if settled {
            Self::confirmed(&mut self.record, &mut turn)?;
        } else if turn.receipt["status"] != "submitted" {
            turn.receipt["status"] = json!("uncertain");
            save_receipt(&turn.receipt)?;
            if let Some(mut stream) = turn.client.take() {
                let _ = respond(&mut stream, &turn.receipt);
            }
        }
        if let Some(mut stream) = turn.interruption.take() {
            let _ = respond(
                &mut stream,
                &json!({"status":if cancelled {"interrupted"} else {"uncertain"}}),
            );
        }
        edit(&mut self.record, |record| {
            record["phase"] = json!("idle");
            record["activity"] = json!("idle");
            if !turn.text.is_empty() {
                event(
                    record,
                    "AgentMessage",
                    &turn.text,
                    &turn.receipt["request_id"],
                );
                record["reply_id"] = turn.receipt["request_id"].clone();
                record["reply_at"] = record["last_event_at"].clone();
            }
            if !settled {
                event(
                    record,
                    "ProviderError",
                    "Antigravity did not confirm the turn",
                    &turn.receipt["request_id"],
                );
            }
        })
    }

    fn control(&mut self, mut stream: UnixStream) -> Result<bool> {
        let timeout = Duration::from_millis(250);
        let request = match frame(&mut stream, Instant::now() + timeout) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        if stream.set_write_timeout(Some(timeout)).is_err() {
            return Ok(false);
        }
        if ["name", "session_id", "run_id", "conversation_id"]
            .into_iter()
            .any(|key| request[key] != self.record[key])
            || binding(string(&self.record, "name"))
                .and_then(|current| {
                    verify(&current, &self.record)?;
                    if current["agy_worker"] == self.record["agy_worker"] {
                        Ok(())
                    } else {
                        Err("Antigravity worker changed".into())
                    }
                })
                .is_err()
        {
            let _ = respond(
                &mut stream,
                &disposition("refused", "The Antigravity session changed"),
            );
            return Ok(false);
        }
        match string(&request, "action") {
            "send" => self.begin(&request, stream)?,
            "interrupt" => {
                let Some(turn) = self.turn.as_mut().filter(|turn| {
                    turn.interruption.is_none()
                        && request["turn_started"] == self.record["turn_started"]
                }) else {
                    let _ = respond(
                        &mut stream,
                        &disposition("refused", "The Antigravity turn changed"),
                    );
                    return Ok(false);
                };
                self.native.notify(
                    "session/cancel",
                    json!({"sessionId":self.record["conversation_id"]}),
                )?;
                turn.interruption = Some(stream);
            }
            "pause" if self.turn.is_none() && self.record["phase"] == "idle" => {
                let until = Instant::now() + Duration::from_secs(5);
                let closed = self.native.call(
                    "session/close",
                    json!({"sessionId":self.record["conversation_id"]}),
                    until,
                )?;
                if !matches!(closed, Reply::Result(_)) {
                    let _ = respond(
                        &mut stream,
                        &disposition("refused", "Antigravity refused to close this session"),
                    );
                    return Ok(false);
                }
                self.native.finish(until)?;
                edit(&mut self.record, |record| {
                    record["agy_provider"] = Value::Null;
                    record["agy_socket"] = Value::Null;
                    record["state"] = json!("paused");
                    record["phase"] = json!("idle");
                    record["activity"] = json!("idle");
                })?;
                let _ = respond(&mut stream, &json!({"status":"stopped"}));
                return Ok(true);
            }
            _ => {
                let _ = respond(
                    &mut stream,
                    &disposition("refused", "This Antigravity action is unavailable"),
                );
            }
        }
        Ok(false)
    }

    fn run(&mut self, listener: &UnixListener) -> Result<()> {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if self.control(stream)? {
                        return Ok(());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return Err("Antigravity control listener failed".into()),
            }
            if let Some(frame) = self.native.receive(Duration::from_millis(20))? {
                self.provider_frame(frame)?;
            }
            if let Some(turn) = self.turn.as_mut() {
                if Instant::now() >= turn.confirm_by {
                    if let Some(mut stream) = turn.client.take() {
                        let mut uncertain = turn.receipt.clone();
                        uncertain["status"] = json!("uncertain");
                        let _ = respond(&mut stream, &uncertain);
                    }
                }
                if Instant::now() >= turn.ends_by {
                    return Err("Antigravity turn outcome remains unconfirmed".into());
                }
            }
        }
    }
}

pub(in crate::state) fn run(args: &[String]) -> Result<i32> {
    let [name, run] = args else {
        return Err("Missing Antigravity worker binding".into());
    };
    let folder = tempfile::Builder::new()
        .prefix("hgs-agy-")
        .tempdir()
        .map_err(|_| "Cannot create a private Antigravity control directory")?;
    crate::platform::restrict_dir(folder.path())
        .map_err(|_| "Cannot protect Antigravity control directory")?;
    let socket = folder.path().join("control");
    let listener =
        UnixListener::bind(&socket).map_err(|_| "Cannot bind Antigravity control endpoint")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Cannot bound Antigravity control listener")?;
    let mut record = {
        let _guard = state_guard()?;
        let mut record = binding(name)?;
        if record["run_id"] != run.as_str()
            || record["state"] != "starting"
            || !record["agy_worker"].is_null()
        {
            return Err("Original Antigravity worker was already selected".into());
        }
        let pid = std::process::id();
        record["agy_worker"] = process_pin(pid)?;
        record["agy_socket"] = json!(socket);
        write(&mut record)?;
        record
    };
    let started = (|| -> Result<Option<Client>> {
        locations(&record)?;
        let cwd = PathBuf::from(string(&record, "cwd"));
        let binary = executable().ok_or("Antigravity CLI is unavailable")?;
        // Publish the fence before spawn. If child identity cannot be
        // published, the original worker's exit alone proves nothing about
        // the provider, including a child that ignores stdin closure.
        edit(&mut record, |r| {
            r["agy_provider"] = json!({"spawn_unconfirmed":true})
        })?;
        let mut native = match Client::start(&binary, &cwd) {
            Ok(child) => child,
            Err(error) => {
                edit(&mut record, |r| r["agy_provider"] = Value::Null)?;
                return Err(error);
            }
        };
        let adopted = process_pin(native.pid())
            .and_then(|provider| edit(&mut record, |r| r["agy_provider"] = provider));
        if let Err(error) = adopted {
            // Only the original child handle can clear unpinned custody here.
            if native
                .finish(Instant::now() + Duration::from_millis(250))
                .is_ok()
            {
                let _ = edit(&mut record, |r| r["agy_provider"] = Value::Null);
            }
            return Err(error);
        }
        native.prepare_input()?;
        let until = Instant::now() + Duration::from_secs(6);
        let initialized = result(native.call(
            "initialize",
            json!({"protocolVersion":1,"clientCapabilities":{},
            "clientInfo":{"name":"Zerus","version":env!("CARGO_PKG_VERSION")}}),
            until,
        )?)?;
        if initialized["protocolVersion"] != 1
            || initialized["agentInfo"]["name"] != "Antigravity ACP Server"
            || !initialized["agentCapabilities"]["sessionCapabilities"]["resume"].is_object()
            || !initialized["agentCapabilities"]["sessionCapabilities"]["close"].is_object()
        {
            return Err("Antigravity did not establish its required native capabilities".into());
        }
        let conversation = string(&record, "conversation_id").to_owned();
        let (method, params) = if conversation.is_empty() {
            ("session/new", json!({"cwd":cwd,"mcpServers":[]}))
        } else {
            (
                "session/resume",
                json!({"sessionId":conversation,"cwd":cwd,"mcpServers":[]}),
            )
        };
        let proof = native.call(method, params, until)?;
        if matches!(proof, Reply::AuthRequired) {
            native.finish(until)?;
            edit(&mut record, |r| {
                r["agy_provider"] = Value::Null;
                r["agy_socket"] = Value::Null;
                r["state"] = json!("stopped");
                r["auth_required"] = json!(true);
                r["agy_startup_refused"] = json!(conversation.is_empty());
                r["phase"] = json!("input");
            })?;
            return Ok(None);
        }
        let proof = result(proof)?;
        let next = string(&proof, "sessionId");
        if !session_id::canonical(next) || !conversation.is_empty() && conversation != next {
            return Err("Antigravity returned a different conversation identity".into());
        }
        edit(&mut record, |r| {
            if !string(r, "agy_restore_archive").is_empty() {
                r["agy_restore_confirmed"] = r["run_id"].clone();
            }
            r["conversation_id"] = json!(next);
            r["state"] = json!("running");
            r["phase"] = json!("idle");
            r["activity"] = json!("idle");
        })?;
        Ok(Some(native))
    })();
    match started {
        Ok(Some(native)) => {
            let mut owner = Owner {
                native,
                record,
                turn: None,
            };
            if let Err(error) = owner.run(&listener) {
                drop(owner.native);
                failed(&mut owner.record, &error)?;
                return Err(error);
            }
        }
        Ok(None) => {}
        Err(error) => {
            failed(&mut record, &error)?;
            return Err(error);
        }
    }
    Ok(0)
}

fn failed(record: &mut Value, error: &str) -> Result<()> {
    edit(record, |r| {
        r["agy_socket"] = Value::Null;
        r["state"] = json!("unknown");
        r["phase"] = json!("unknown");
        r["activity"] = json!("unknown");
        r["error"] = json!(error);
    })
}
