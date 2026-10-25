//! The `/rpc` endpoint: one request in, one reply out.
//!
//! Every handler that changes state runs inside one store transaction and
//! publishes its events right after the commit, still under the store's lock:
//! events therefore reach subscribers in commit order. Room mutations share
//! that sequence in [`in_room`].

use std::time::Instant;

use rusqlite::Connection;
use schwitzer_proto::{
    ApiError, DebugId, DebugSession, Event, MAX_RPC_BYTES, Member, MemberId, PROTOCOL_VERSION,
    Project, RPC_TIMEOUT, Reply, Request, Response, RoomId, RoomToken, SessionInfo, Stage, Task,
    caps, from_bytes, now_ms, to_bytes, valid_stream_name,
};
use tokio::time::timeout;
use weida::{ErrorCode, IncomingRequest, Replier, TransferMeta};

use crate::auth;
use crate::hub::Hub;
use crate::limit;
use crate::store::{self, R};
use crate::{Shared, State};

/// Accepts requests until the replier closes, each in its own task.
pub async fn serve(state: State, replier: Replier) {
    while let Ok(request) = replier.accept().await {
        tokio::spawn(handle(state.clone(), request));
    }
}

async fn handle(state: State, mut request: IncomingRequest) {
    let caller = match auth::caller(&request) {
        Ok(caller) => caller,
        Err(e) => {
            respond(request, Err(e)).await;
            return;
        }
    };
    let body = timeout(RPC_TIMEOUT, request.body().read_capped(MAX_RPC_BYTES)).await;
    let reply: Reply = match body {
        Ok(Ok(body)) => match from_bytes::<Request>(&body) {
            Ok(req) => dispatch(&state, caller, req).await,
            Err(_) => Err(ApiError::Invalid("malformed request".into())),
        },
        Ok(Err(weida::Error::TooLarge { .. })) => {
            Err(ApiError::Invalid("request too large".into()))
        }
        Ok(Err(_)) | Err(_) => {
            request.refuse(ErrorCode::Rejected).await;
            return;
        }
    };
    respond(request, reply).await;
}

/// Sends `reply` as the answer to `request`.
async fn respond(request: IncomingRequest, reply: Reply) {
    let bytes = to_bytes(&reply);
    let bytes = if bytes.len() > MAX_RPC_BYTES {
        to_bytes::<Reply>(&Err(ApiError::Internal))
    } else {
        bytes
    };
    let sent = timeout(RPC_TIMEOUT, async {
        let mut out = request.reply(TransferMeta::default()).await?;
        out.write_all(&bytes).await?;
        out.finish()
    })
    .await;
    match sent {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::debug!("reply failed: {e}"),
        Err(_) => tracing::debug!("reply timed out"),
    }
}

/// A text field: trimmed, at most `cap` bytes, and non-empty if `required`.
fn text(field: &str, value: &str, cap: usize, required: bool) -> Result<String, ApiError> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(ApiError::Invalid(format!("{field} is empty")));
    }
    if value.len() > cap {
        return Err(ApiError::Invalid(format!(
            "{field} is too long (at most {cap} bytes)"
        )));
    }
    Ok(value.to_owned())
}

/// Task numbers to wait for: ascending, without duplicates, at most `caps::WAITS_FOR`.
fn waits(mut list: Vec<u64>) -> Result<Vec<u64>, ApiError> {
    list.sort_unstable();
    list.dedup();
    if list.len() > caps::WAITS_FOR {
        return Err(ApiError::Invalid(format!(
            "a task waits for at most {} tasks",
            caps::WAITS_FOR
        )));
    }
    Ok(list)
}

/// Display metadata: trimmed and cut to `cap` bytes at a character boundary.
fn clamp(value: &str, cap: usize) -> String {
    caps::cut(value.trim(), cap).to_owned()
}

/// Rooms one member may belong to.
const MAX_ROOMS_PER_MEMBER: u64 = 50;

fn too_many_rooms() -> ApiError {
    ApiError::Invalid(format!(
        "you are in {MAX_ROOMS_PER_MEMBER} rooms already; leave one to create or join another"
    ))
}

/// Whether `req` spends from the caller's request budget: everything except `Hello` and
/// `JoinRoom`, which every reconnect sends (one `JoinRoom` per room), and `DebugLog`, which
/// spends from the log budget instead.
fn limited(req: &Request) -> bool {
    !matches!(
        req,
        Request::Hello { .. } | Request::JoinRoom { .. } | Request::DebugLog { .. }
    )
}

async fn dispatch(state: &State, caller: MemberId, req: Request) -> Reply {
    if limited(&req) {
        state.limits.take(caller, Instant::now()).map_err(|wait| {
            ApiError::Invalid(format!(
                "too many requests, try again in {} s",
                limit::wait_secs(wait)
            ))
        })?;
    }
    match req {
        Request::Hello { protocol, name } => {
            if protocol != PROTOCOL_VERSION {
                return Err(ApiError::Protocol);
            }
            let name = text("name", &name, caps::MEMBER_NAME, true)?;
            set_name(state, caller, name, false).await?;
            Ok(Response::Hello { member: caller })
        }
        Request::SetName { name } => {
            let name = text("name", &name, caps::MEMBER_NAME, true)?;
            set_name(state, caller, name, true).await?;
            Ok(Response::Ok)
        }
        Request::CreateRoom { name } => {
            let name = text("room name", &name, caps::ROOM_NAME, true)?;
            create_room(state, caller, name).await
        }
        Request::JoinRoom { token } => join_room(state, caller, token).await,
        Request::LeaveRoom { room } => leave_room(state, caller, room).await,
        Request::Propose {
            room,
            project,
            title,
            body,
            waits_for,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            let waits_for = waits(waits_for)?;
            mutate(state, caller, room, move |conn, now| {
                let task = store::create_task(
                    conn,
                    &room,
                    &caller,
                    project,
                    Stage::Proposed,
                    &title,
                    &body,
                    now,
                )?;
                if waits_for.is_empty() {
                    return Ok(task);
                }
                store::set_waits(conn, &room, task.number, &waits_for, now)
            })
            .await
        }
        Request::AddIdea {
            room,
            project,
            title,
            body,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            mutate(state, caller, room, move |conn, now| {
                store::create_task(
                    conn,
                    &room,
                    &caller,
                    project,
                    Stage::Idea,
                    &title,
                    &body,
                    now,
                )
            })
            .await
        }
        Request::ProposeIdea {
            room,
            task,
            title,
            body,
            waits_for,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            let waits_for = waits(waits_for)?;
            mutate(state, caller, room, move |conn, now| {
                store::propose_idea(conn, &room, task, &title, &body, &waits_for, now)
            })
            .await
        }
        Request::EditTask {
            room,
            task,
            title,
            body,
            plan,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            let plan = text("plan", &plan, caps::PLAN, false)?;
            mutate(state, caller, room, move |conn, now| {
                store::edit_task(conn, &room, task, &title, &body, &plan, now)
            })
            .await
        }
        Request::SetPriority {
            room,
            task,
            priority,
        } => {
            mutate(state, caller, room, move |conn, now| {
                store::set_priority(conn, &room, task, priority, now)
            })
            .await
        }
        Request::Decide {
            room,
            task,
            decision,
        } => {
            mutate(state, caller, room, move |conn, now| {
                store::decide(conn, &room, task, decision, now)
            })
            .await
        }
        Request::Claim {
            room,
            work,
            project,
            task,
            agent,
        } => {
            let agent = text("agent", &agent, caps::AGENT_LABEL, false)?;
            mutate_claim(state, caller, room, move |conn, now| {
                store::claim(conn, &room, &caller, work, project, task, &agent, now)
            })
            .await
        }
        Request::SubmitPlan { room, task, plan } => {
            let plan = text("plan", &plan, caps::PLAN, false)?;
            mutate(state, caller, room, move |conn, now| {
                store::submit_plan(conn, &room, &caller, task, &plan, now)
            })
            .await
        }
        Request::Finish { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            mutate(state, caller, room, move |conn, now| {
                store::finish(conn, &room, &caller, task, &note, now)
            })
            .await
        }
        Request::Unclaim { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            mutate(state, caller, room, move |conn, now| {
                store::unclaim(conn, &room, &caller, task, &note, now)
            })
            .await
        }
        Request::AddNote {
            room,
            task,
            text: note,
            agent,
        } => {
            let note = text("text", &note, caps::NOTE, true)?;
            let agent = match agent {
                Some(label) => Some(text("agent", &label, caps::AGENT_LABEL, false)?)
                    .filter(|label| !label.is_empty()),
                None => None,
            };
            mutate(state, caller, room, move |conn, now| {
                store::add_note(conn, &room, &caller, task, &note, agent.as_deref(), now)
            })
            .await
        }
        Request::PublishSessions { room, sessions } => {
            publish_sessions(state, caller, room, sessions).await
        }
        Request::CreateProject { room, name } => {
            let name = text("project name", &name, caps::PROJECT_NAME, true)?;
            mutate_project(state, caller, room, move |conn, now| {
                store::create_project(conn, &room, &name, now)
            })
            .await
        }
        Request::RenameProject {
            room,
            project,
            name,
        } => {
            let name = text("project name", &name, caps::PROJECT_NAME, true)?;
            mutate_project(state, caller, room, move |conn, _| {
                store::rename_project(conn, &room, project, &name)
            })
            .await
        }
        Request::MoveTask {
            room,
            task,
            project,
        } => {
            mutate(state, caller, room, move |conn, now| {
                store::move_task(conn, &room, task, project, now)
            })
            .await
        }
        Request::DeleteProject { room, project } => {
            delete_project(state, caller, room, project).await
        }
        Request::SetWaits {
            room,
            task,
            waits_for,
        } => {
            let waits_for = waits(waits_for)?;
            mutate(state, caller, room, move |conn, now| {
                store::set_waits(conn, &room, task, &waits_for, now)
            })
            .await
        }
        Request::DebugStart { room } => debug_start(state, caller, room).await,
        Request::DebugJoin { room, session } => debug_join(state, caller, room, session).await,
        Request::DebugLeave { room, session } => debug_leave(state, caller, room, session).await,
        Request::DebugLog {
            room,
            session,
            stream,
            offset,
            data,
        } => debug_log(state, caller, room, session, stream, offset, data),
        Request::DebugInstruct {
            room,
            session,
            to,
            text: instruction,
        } => {
            let instruction = text("instruction", &instruction, caps::INSTRUCTION, true)?;
            debug_instruct(state, caller, room, session, to, instruction).await
        }
    }
}

/// Runs `change` in one store transaction as a member of `room`, commits, then
/// calls `publish` with the room's member ids and what `change` returned.
///
/// This is the one place that keeps the module's ordering invariant: `publish`
/// runs right after the commit, inside the store's lock (the closure of
/// `Store::run`), so events reach subscribers in commit order. `change` runs
/// before the member ids are read; a refusal from it rolls the transaction back
/// and publishes nothing.
async fn in_room<T, C, P>(
    state: &State,
    caller: MemberId,
    room: RoomId,
    change: C,
    publish: P,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    C: FnOnce(&Shared, &Connection, u64) -> R<T> + Send + 'static,
    P: FnOnce(&Hub, &[MemberId], &T) + Send + 'static,
{
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            let out = change(&st, &tx, now_ms())?;
            let members = store::room_member_ids(&tx, &room)?;
            tx.commit()?;
            publish(&st.hub, &members, &out);
            Ok(out)
        })
        .await
}

/// Runs a task mutation as a member of `room`, publishes the resulting task to
/// the room's members and replies with it.
async fn mutate<F>(state: &State, caller: MemberId, room: RoomId, f: F) -> Reply
where
    F: FnOnce(&Connection, u64) -> R<Task> + Send + 'static,
{
    let task = in_room(
        state,
        caller,
        room,
        move |st, conn, now| {
            let task = f(conn, now)?;
            crate::require_task_fits(&task)?;
            st.require_room_fits(conn, &room)?;
            Ok(task)
        },
        move |hub, members, task| {
            hub.publish(
                members,
                &Event::Task {
                    room,
                    task: task.clone(),
                },
            );
        },
    )
    .await?;
    Ok(Response::Task(task))
}

/// Claims a task as a member of `room`. A claim that finds no free task changes
/// nothing and publishes nothing; otherwise the task goes to the room's members.
async fn mutate_claim<F>(state: &State, caller: MemberId, room: RoomId, f: F) -> Reply
where
    F: FnOnce(&Connection, u64) -> R<Option<Task>> + Send + 'static,
{
    let claimed = in_room(
        state,
        caller,
        room,
        move |st, conn, now| {
            let task = f(conn, now)?;
            if let Some(task) = &task {
                crate::require_task_fits(task)?;
                st.require_room_fits(conn, &room)?;
            }
            Ok(task)
        },
        move |hub, members, task| {
            if let Some(task) = task {
                hub.publish(
                    members,
                    &Event::Task {
                        room,
                        task: task.clone(),
                    },
                );
            }
        },
    )
    .await?;
    Ok(Response::Claimed(claimed))
}

/// Runs a project mutation as a member of `room`, publishes the resulting
/// project to the room's members and replies with it.
async fn mutate_project<F>(state: &State, caller: MemberId, room: RoomId, f: F) -> Reply
where
    F: FnOnce(&Connection, u64) -> R<Project> + Send + 'static,
{
    let project = in_room(
        state,
        caller,
        room,
        move |st, conn, now| {
            let project = f(conn, now)?;
            st.require_room_fits(conn, &room)?;
            Ok(project)
        },
        move |hub, members, project| {
            hub.publish(
                members,
                &Event::Project {
                    room,
                    project: project.clone(),
                },
            );
        },
    )
    .await?;
    Ok(Response::Project(project))
}

/// Deletes an empty project as a member of `room`, then tells the room's members.
async fn delete_project(state: &State, caller: MemberId, room: RoomId, project: u64) -> Reply {
    in_room(
        state,
        caller,
        room,
        move |_, conn, _| store::delete_project(conn, &room, project),
        move |hub, members, ()| hub.publish(members, &Event::ProjectDeleted { room, project }),
    )
    .await?;
    Ok(Response::Ok)
}

/// Stores the caller's name and tells their rooms; `always` also when it did
/// not change.
async fn set_name(
    state: &State,
    caller: MemberId,
    name: String,
    always: bool,
) -> Result<(), ApiError> {
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let changed = store::upsert_person(conn, &caller, &name)?;
            if changed || always {
                st.announce(conn, &caller)?;
            }
            Ok(())
        })
        .await
}

async fn create_room(state: &State, caller: MemberId, name: String) -> Reply {
    let token = RoomToken::random();
    let id = RoomId::of(&token);
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            if store::membership_count(&tx, &caller)? >= MAX_ROOMS_PER_MEMBER {
                return Err(too_many_rooms().into());
            }
            store::create_room(&tx, &id, &name, &caller, now_ms())?;
            let room = st.room_state(&tx, &id)?;
            tx.commit()?;
            st.hub.publish(&[caller], &Event::Room(room));
            Ok(())
        })
        .await?;
    Ok(Response::Room { id, token })
}

async fn join_room(state: &State, caller: MemberId, token: RoomToken) -> Reply {
    let id = RoomId::of(&token);
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            if !store::room_exists(&tx, &id)? {
                return Err(ApiError::NoSuchRoom.into());
            }
            let added = store::add_member(&tx, &id, &caller, now_ms())?;
            if !added {
                return Ok(());
            }
            if store::membership_count(&tx, &caller)? > MAX_ROOMS_PER_MEMBER {
                return Err(too_many_rooms().into());
            }
            let snapshot = Event::Room(st.room_state(&tx, &id)?);
            crate::require_room_fits(&snapshot)?;
            let others: Vec<MemberId> = store::room_member_ids(&tx, &id)?
                .into_iter()
                .filter(|m| *m != caller)
                .collect();
            let member = Member {
                id: caller,
                name: store::person_name(&tx, &caller)?,
                online: st.hub.is_online(&caller),
            };
            tx.commit()?;
            st.hub.publish(&[caller], &snapshot);
            st.hub.publish(&others, &Event::Member { room: id, member });
            Ok(())
        })
        .await?;
    Ok(Response::Room { id, token })
}

async fn leave_room(state: &State, caller: MemberId, room: RoomId) -> Reply {
    in_room(
        state,
        caller,
        room,
        move |st, conn, _| {
            store::remove_member(conn, &room, &caller)?;
            st.sessions.clear(&room, &caller);
            let member = Member {
                id: caller,
                name: store::person_name(conn, &caller)?,
                online: false,
            };
            Ok((member, st.debug.remove_member(&room, &caller)))
        },
        move |hub, others, (member, debug)| {
            hub.publish(&[caller], &Event::Left { room });
            hub.publish(
                others,
                &Event::Member {
                    room,
                    member: member.clone(),
                },
            );
            hub.publish(
                others,
                &Event::Sessions {
                    room,
                    member: caller,
                    sessions: Vec::new(),
                },
            );
            if let Some(session) = debug {
                hub.publish(
                    others,
                    &Event::Debug {
                        room,
                        session: session.clone(),
                    },
                );
            }
        },
    )
    .await?;
    Ok(Response::Ok)
}

async fn publish_sessions(
    state: &State,
    caller: MemberId,
    room: RoomId,
    sessions: Vec<SessionInfo>,
) -> Reply {
    if sessions.len() > caps::SESSIONS_PER_PUBLISH {
        return Err(ApiError::Invalid(format!(
            "sessions has more than {} entries",
            caps::SESSIONS_PER_PUBLISH
        )));
    }
    if sessions
        .iter()
        .any(|s| s.projects.len() > caps::SESSION_PROJECTS)
    {
        return Err(ApiError::Invalid(format!(
            "a session names more than {} projects",
            caps::SESSION_PROJECTS
        )));
    }
    let sessions: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|s| SessionInfo {
            id: clamp(&s.id, caps::SESSION_TITLE),
            title: clamp(&s.title, caps::SESSION_TITLE),
            project: clamp(&s.project, caps::SESSION_TITLE),
            model: clamp(&s.model, caps::SESSION_TITLE),
            ..s
        })
        .collect();
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            store::require_member(conn, &room, &caller)?;
            if !sessions.is_empty() {
                let mut snapshot = st.room_state(conn, &room)?;
                snapshot.sessions.retain(|(m, _)| *m != caller);
                snapshot.sessions.push((caller, sessions.clone()));
                crate::require_room_fits(&Event::Room(snapshot))?;
            }
            st.sessions.set(room, caller, sessions.clone());
            let members = store::room_member_ids(conn, &room)?;
            st.hub.publish(
                &members,
                &Event::Sessions {
                    room,
                    member: caller,
                    sessions,
                },
            );
            Ok(())
        })
        .await?;
    Ok(Response::Ok)
}

/// Whom a debug event goes to.
enum Audience {
    /// Every member of the room.
    Room,
    /// These members only.
    Only(Vec<MemberId>),
}

/// Runs `change` as a member of `room`, then publishes the event it returns to its
/// audience. The debug sessions live in memory; the store only checks membership and lists
/// the members, and its lock keeps the events in order.
async fn in_debug<T, C>(
    state: &State,
    caller: MemberId,
    room: RoomId,
    change: C,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    C: FnOnce(&Shared) -> Result<(T, Option<(Audience, Event)>), ApiError> + Send + 'static,
{
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            store::require_member(conn, &room, &caller)?;
            let (out, event) = change(&st)?;
            if let Some((audience, event)) = event {
                let recipients = match audience {
                    Audience::Room => store::room_member_ids(conn, &room)?,
                    Audience::Only(members) => members,
                };
                st.hub.publish(&recipients, &event);
            }
            Ok(out)
        })
        .await
}

fn debug_event(room: RoomId, session: Option<DebugSession>) -> Option<(Audience, Event)> {
    Some((Audience::Room, Event::Debug { room, session }))
}

async fn debug_start(state: &State, caller: MemberId, room: RoomId) -> Reply {
    let session = in_debug(state, caller, room, move |st| {
        let s = st.debug.start(room, caller, now_ms())?;
        Ok((s.clone(), debug_event(room, Some(s))))
    })
    .await?;
    Ok(Response::Debug(session))
}

async fn debug_join(state: &State, caller: MemberId, room: RoomId, session: DebugId) -> Reply {
    let session = in_debug(state, caller, room, move |st| {
        let (s, changed) = st.debug.join(&room, session, caller)?;
        let event = changed
            .then(|| debug_event(room, Some(s.clone())))
            .flatten();
        Ok((s, event))
    })
    .await?;
    Ok(Response::Debug(session))
}

async fn debug_leave(state: &State, caller: MemberId, room: RoomId, session: DebugId) -> Reply {
    in_debug(state, caller, room, move |st| {
        let after = st.debug.leave(&room, session, caller)?;
        Ok(((), debug_event(room, after)))
    })
    .await?;
    Ok(Response::Ok)
}

async fn debug_instruct(
    state: &State,
    caller: MemberId,
    room: RoomId,
    session: DebugId,
    to: Vec<MemberId>,
    text: String,
) -> Reply {
    let instruction = in_debug(state, caller, room, move |st| {
        let (instruction, publish) =
            st.debug
                .instruct(&room, session, caller, to, text, now_ms())?;
        let event = Event::DebugInstruction {
            room,
            session,
            instruction: instruction.clone(),
        };
        Ok((instruction, Some((Audience::Only(publish), event))))
    })
    .await?;
    Ok(Response::DebugInstruction(instruction))
}

/// Relays log bytes to the session's other members. Membership of the session implies
/// membership of the room (leaving the room leaves the session), so the store is not asked.
fn debug_log(
    state: &State,
    caller: MemberId,
    room: RoomId,
    session: DebugId,
    stream: String,
    offset: u64,
    data: Vec<u8>,
) -> Reply {
    if !valid_stream_name(&stream) {
        return Err(ApiError::Invalid("invalid stream name".into()));
    }
    if data.is_empty() || data.len() > caps::DEBUG_CHUNK {
        return Err(ApiError::Invalid(format!(
            "log data must hold 1 to {} bytes",
            caps::DEBUG_CHUNK
        )));
    }
    let targets = state.debug.log_targets(&room, session, &caller)?;
    state
        .log_budget
        .take(caller, data.len(), Instant::now())
        .map_err(|wait| ApiError::LogBudget(limit::wait_secs(wait)))?;
    state.hub.publish(
        &targets,
        &Event::DebugLog {
            room,
            session,
            from: caller,
            stream,
            offset,
            data,
        },
    );
    Ok(Response::Ok)
}
