use super::*;

impl Runtime {
    pub(super) async fn approve(
        &mut self,
        rpc_id: Value,
        reply: Reply,
        tool_call: Value,
    ) -> Result<(), String> {
        let tool_kind = serde_json::from_value::<ToolKind>(tool_call["kind"].clone()).ok();
        if tier_approves(*self.auto_approve.lock(), tool_kind) {
            self.reply(&rpc_id, &decision(&reply, "allow"));
            return Ok(());
        }
        let options = approval_options(&reply);
        self.ask(Pending { rpc_id, reply }, tool_call, options, Vec::new())
            .await
    }

    pub(super) async fn ask(
        &mut self,
        pending: Pending,
        tool_call: Value,
        options: Value,
        questions: Vec<AgentQuestion>,
    ) -> Result<(), String> {
        let request_id = self.permission_ids.fetch_add(1, Ordering::Relaxed);
        let payload = AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            questions,
        };
        if let Err(error) = validate_payload(&payload) {
            log::warn!(target: "zz::agent", "Codex asked something too large to show: {error}");
            if !matches!(pending.reply, Reply::Async { .. }) {
                self.reply(&pending.rpc_id, &decision(&pending.reply, "reject"));
            }
            return Ok(());
        }
        self.permissions.insert(request_id, pending);
        self.emit(payload).await
    }

    pub(super) async fn answer(
        &mut self,
        request_id: u64,
        option_id: Option<&str>,
    ) -> Result<(), String> {
        let Some(pending) = self.permissions.remove(&request_id) else {
            return Ok(());
        };
        let chosen = match &pending.reply {
            Reply::Questions(questions) | Reply::Async { questions, .. } => {
                chosen(questions, option_id)
            }
            _ => None,
        };
        let answer = match &chosen {
            Some(answer) => chat_answers(std::slice::from_ref(answer)),
            None => chat_answer(option_id),
        };
        self.chat.lock().answered(request_id, answer);
        self.emit(AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: option_id.is_none(),
        })
        .await?;
        let result = match (&pending.reply, option_id) {
            (Reply::Async { call, questions }, _) => {
                if let Some(chosen) = &chosen {
                    self.answer_async(call, questions, std::slice::from_ref(chosen));
                }
                return Ok(());
            }
            (Reply::Questions(_), _) => question_answers(chosen.as_slice()),
            (reply, Some(option)) => decision(reply, option),
            (reply, None) => decision(reply, "cancel"),
        };
        self.reply(&pending.rpc_id, &result);
        Ok(())
    }

    pub(super) async fn answer_questions(
        &mut self,
        request_id: u64,
        answers: &[AgentQuestionAnswer],
    ) -> Result<(), String> {
        if self.permissions.contains_key(&request_id) {
            self.chat.lock().answered(request_id, chat_answers(answers));
        }
        let Some(pending) = self.permissions.remove(&request_id) else {
            return Ok(());
        };
        self.emit(AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: false,
        })
        .await?;
        match &pending.reply {
            Reply::Async { call, questions } => self.answer_async(call, questions, answers),
            _ => self.reply(&pending.rpc_id, &question_answers(answers)),
        }
        Ok(())
    }

    fn answer_async(
        &mut self,
        call: &str,
        questions: &[AgentQuestion],
        answers: &[AgentQuestionAnswer],
    ) {
        let steer = self
            .turns
            .turn()
            .and_then(Turn::active_command)
            .map(str::to_owned);
        self.send_answer(question_reply(call, questions, answers), steer);
    }

    pub(super) fn send_answer(&mut self, text: String, steer: Option<String>) {
        let Some(thread) = self.thread_id() else {
            return;
        };
        let input = json!([{ "type": "text", "text": text, "text_elements": [] }]);
        if let Some(turn) = steer {
            self.request(
                "turn/steer",
                &json!({ "threadId": thread, "expectedTurnId": turn, "input": input }),
                Outgoing::Answer {
                    text,
                    steered: true,
                },
            );
            return;
        }
        let mut params = json!({ "threadId": thread, "input": input });
        self.settings.turn_overrides(&mut params);
        self.request(
            "turn/start",
            &params,
            Outgoing::Answer {
                text,
                steered: false,
            },
        );
    }

    pub(super) async fn open_questions(&mut self) -> Result<(), String> {
        for (call, questions) in self.translator.take_asked() {
            if self.asking(&call).is_some() {
                continue;
            }
            let (options, title) = question_options(&questions);
            let tool_call = json!({
                "toolCallId": call,
                "title": title,
                "kind": "other",
                "_meta": { "zz": { "background": true } },
            });
            let reply = Reply::Async {
                call,
                questions: questions.clone(),
            };
            self.ask(
                Pending {
                    rpc_id: Value::Null,
                    reply,
                },
                tool_call,
                options,
                questions,
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn answered_elsewhere(&mut self, updates: &[Value]) -> Result<(), String> {
        for update in updates {
            let (Some(call), Some(answered)) = (
                update["toolCallId"].as_str(),
                update["_meta"]["zz"]["answered"].as_array(),
            ) else {
                continue;
            };
            let Some(request_id) = self.asking(call) else {
                continue;
            };
            self.permissions.remove(&request_id);
            let answers = answered
                .iter()
                .enumerate()
                .map(|(index, entry)| AgentQuestionAnswer {
                    id: format!("q{index}"),
                    answers: entry["answer"]
                        .as_str()
                        .filter(|answer| !answer.is_empty())
                        .map(str::to_owned)
                        .into_iter()
                        .collect(),
                })
                .collect::<Vec<_>>();
            self.chat
                .lock()
                .answered(request_id, chat_answers(&answers));
            self.emit(AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: answers.iter().all(|answer| answer.answers.is_empty()),
            })
            .await?;
        }
        Ok(())
    }

    fn asking(&self, call: &str) -> Option<u64> {
        self.permissions
            .iter()
            .find(|(_, pending)| {
                matches!(&pending.reply, Reply::Async { call: asked, .. } if asked == call)
            })
            .map(|(request_id, _)| *request_id)
    }

    pub(super) async fn cancel_permissions(&mut self) -> Result<(), String> {
        self.cancel_where(|_| true).await
    }

    pub(super) async fn cancel_blocking(&mut self) -> Result<(), String> {
        self.cancel_where(|reply| !matches!(reply, Reply::Async { .. }))
            .await
    }

    async fn cancel_where(&mut self, which: impl Fn(&Reply) -> bool) -> Result<(), String> {
        let cancelled = self
            .permissions
            .iter()
            .filter(|(_, pending)| which(&pending.reply))
            .map(|(request_id, _)| *request_id)
            .collect::<Vec<_>>();
        for request_id in cancelled {
            let Some(pending) = self.permissions.remove(&request_id) else {
                continue;
            };
            let result = match &pending.reply {
                Reply::Async { .. } => None,
                Reply::Questions(_) => Some(question_answers(&[])),
                reply => Some(decision(reply, "cancel")),
            };
            if let Some(result) = result {
                self.reply(&pending.rpc_id, &result);
            }
            self.emit(AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: true,
            })
            .await?;
        }
        Ok(())
    }
}

pub fn approval(method: &str, params: &Value) -> Option<(Reply, Value)> {
    let item = params["itemId"].as_str().unwrap_or_default();
    let reason = params["reason"]
        .as_str()
        .filter(|reason| !reason.is_empty());
    let (reply, title, kind, reason) = match method {
        "item/commandExecution/requestApproval" | "execCommandApproval" => {
            let command = params["command"]
                .as_str()
                .map(translate::shell_script)
                .or_else(|| {
                    params["command"].as_array().map(|parts| {
                        parts
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                })
                .unwrap_or_else(|| "Run a command".to_owned());
            let reply = if method == "execCommandApproval" {
                Reply::Legacy
            } else {
                Reply::Command
            };
            (reply, command, "execute", reason)
        }
        "item/fileChange/requestApproval" | "applyPatchApproval" => {
            let reply = if method == "applyPatchApproval" {
                Reply::Legacy
            } else {
                Reply::FileChange
            };
            (reply, "Apply the changes".to_owned(), "edit", reason)
        }
        "item/permissions/requestApproval" => (
            Reply::Permissions(params["permissions"].clone()),
            reason.unwrap_or("Grant more permissions").to_owned(),
            "other",
            None,
        ),
        _ => return None,
    };
    let mut tool_call = json!({ "toolCallId": item, "title": title, "kind": kind });
    if let Some(reason) = reason {
        tool_call["content"] =
            json!([{ "type": "content", "content": { "type": "text", "text": reason } }]);
    }
    Some((reply, tool_call))
}

pub fn approval_options(reply: &Reply) -> Value {
    let mut options = vec![option("allow", "Allow", "allow_once")];
    if !matches!(reply, Reply::Permissions(_)) {
        options.push(option(
            "allow_always",
            "Always allow this session",
            "allow_always",
        ));
    }
    options.push(option("reject", "Reject", "reject_once"));
    Value::Array(options)
}

pub fn user_input(params: &Value) -> (Vec<AgentQuestion>, Value, String) {
    let questions = params["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|question| AgentQuestion {
            id: question["id"].as_str().unwrap_or_default().to_owned(),
            header: question["header"]
                .as_str()
                .filter(|header| !header.is_empty())
                .map(str::to_owned),
            question: question["question"]
                .as_str()
                .unwrap_or("Question")
                .to_owned(),
            options: question["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|choice| AgentQuestionOption {
                    label: choice["label"].as_str().unwrap_or_default().to_owned(),
                    description: choice["description"]
                        .as_str()
                        .filter(|text| !text.is_empty())
                        .map(str::to_owned),
                })
                .collect(),
            multi_select: false,
            allow_other: question["isOther"] == true,
            secret: question["isSecret"] == true,
        })
        .collect::<Vec<_>>();
    let (options, title) = question_options(&questions);
    (questions, options, title)
}

pub fn question_options(questions: &[AgentQuestion]) -> (Value, String) {
    match questions {
        [only] => (
            only.options
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    option(&format!("answer-{index}"), &choice.label, "allow_once")
                })
                .collect(),
            only.question.clone(),
        ),
        _ => (Value::Array(Vec::new()), "Codex has questions".to_owned()),
    }
}

pub fn decision(reply: &Reply, option: &str) -> Value {
    match reply {
        Reply::Command | Reply::FileChange => json!({
            "decision": match option {
                "allow" => "accept",
                "allow_always" => "acceptForSession",
                "cancel" => "cancel",
                _ => "decline",
            },
        }),
        Reply::Legacy => json!({
            "decision": match option {
                "allow" => json!("approved"),
                "allow_always" => json!("approved_for_session"),
                "cancel" => json!("abort"),
                _ => json!({ "denied": { "rejection": "The user rejected this." } }),
            },
        }),
        Reply::Permissions(requested) => match option {
            "allow" | "allow_always" => json!({
                "permissions": {
                    "network": requested["network"],
                    "fileSystem": requested["fileSystem"],
                },
                "scope": "turn",
            }),
            _ => json!({ "permissions": {}, "scope": "turn" }),
        },
        Reply::Questions(_) | Reply::Async { .. } => question_answers(&[]),
    }
}

pub fn chosen(questions: &[AgentQuestion], option: Option<&str>) -> Option<AgentQuestionAnswer> {
    let index = option?.strip_prefix("answer-")?.parse::<usize>().ok()?;
    let question = questions.first()?;
    Some(AgentQuestionAnswer {
        id: question.id.clone(),
        answers: vec![question.options.get(index)?.label.clone()],
    })
}

pub fn question_answers(answers: &[AgentQuestionAnswer]) -> Value {
    let map = answers
        .iter()
        .map(|answer| (answer.id.clone(), json!({ "answers": answer.answers })))
        .collect::<Map<_, _>>();
    json!({ "answers": map })
}
