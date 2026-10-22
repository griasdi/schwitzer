//! The `/rpc` endpoint: one request in, one reply out.
//!
//! Every handler that changes state runs inside one store transaction and
//! publishes its events right after the commit, still under the store's lock:
//! events therefore reach subscribers in commit order.

use std::time::Instant;

use rusqlite::Connection;
use schwitzer_proto::{
    ApiError, Event, MAX_RPC_BYTES, Member, MemberId, PROTOCOL_VERSION, Project, RPC_TIMEOUT,
    Reply, Request, Response, RoomId, RoomToken, SessionInfo, Stage, Task, caps, from_bytes,
    now_ms, to_bytes,
};
use tokio::time::timeout;
use weida::{ErrorCode, IncomingRequest, Replier, TransferMeta};

use crate::State;
use crate::auth;
use crate::store::{self, R};

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
/// `JoinRoom`, which every reconnect sends (one `JoinRoom` per room).
fn limited(req: &Request) -> bool {
    !matches!(req, Request::Hello { .. } | Request::JoinRoom { .. })
}

async fn dispatch(state: &State, caller: MemberId, req: Request) -> Reply {
    if limited(&req) {
        state.limits.take(caller, Instant::now()).map_err(|wait| {
            ApiError::Invalid(format!(
                "too many requests, try again in {} s",
                wait.as_millis().div_ceil(1000)
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
            task_reply(
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
                        return Ok(Some(task));
                    }
                    store::set_waits(conn, &room, task.number, &waits_for, now).map(Some)
                })
                .await,
            )
        }
        Request::AddIdea {
            room,
            project,
            title,
            body,
        } => {
            let title = text("title", &title, caps::TITLE, true)?;
            let body = text("body", &body, caps::BODY, false)?;
            task_reply(
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
                    .map(Some)
                })
                .await,
            )
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
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::propose_idea(conn, &room, task, &title, &body, &waits_for, now).map(Some)
                })
                .await,
            )
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
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::edit_task(conn, &room, task, &title, &body, &plan, now).map(Some)
                })
                .await,
            )
        }
        Request::SetPriority {
            room,
            task,
            priority,
        } => task_reply(
            mutate(state, caller, room, move |conn, now| {
                store::set_priority(conn, &room, task, priority, now).map(Some)
            })
            .await,
        ),
        Request::Decide {
            room,
            task,
            decision,
        } => task_reply(
            mutate(state, caller, room, move |conn, now| {
                store::decide(conn, &room, task, decision, now).map(Some)
            })
            .await,
        ),
        Request::Claim {
            room,
            work,
            project,
            task,
            agent,
        } => {
            let agent = text("agent", &agent, caps::AGENT_LABEL, false)?;
            let claimed = mutate(state, caller, room, move |conn, now| {
                store::claim(conn, &room, &caller, work, project, task, &agent, now)
            })
            .await?;
            Ok(Response::Claimed(claimed))
        }
        Request::SubmitPlan { room, task, plan } => {
            let plan = text("plan", &plan, caps::PLAN, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::submit_plan(conn, &room, &caller, task, &plan, now).map(Some)
                })
                .await,
            )
        }
        Request::Finish { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::finish(conn, &room, &caller, task, &note, now).map(Some)
                })
                .await,
            )
        }
        Request::Unclaim { room, task, note } => {
            let note = text("note", &note, caps::NOTE, false)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::unclaim(conn, &room, &caller, task, &note, now).map(Some)
                })
                .await,
            )
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
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::add_note(conn, &room, &caller, task, &note, agent.as_deref(), now)
                        .map(Some)
                })
                .await,
            )
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
        } => task_reply(
            mutate(state, caller, room, move |conn, now| {
                store::move_task(conn, &room, task, project, now).map(Some)
            })
            .await,
        ),
        Request::DeleteProject { room, project } => {
            delete_project(state, caller, room, project).await
        }
        Request::SetWaits {
            room,
            task,
            waits_for,
        } => {
            let waits_for = waits(waits_for)?;
            task_reply(
                mutate(state, caller, room, move |conn, now| {
                    store::set_waits(conn, &room, task, &waits_for, now).map(Some)
                })
                .await,
            )
        }
    }
}

fn task_reply(result: Result<Option<Task>, ApiError>) -> Reply {
    match result? {
        Some(task) => Ok(Response::Task(task)),
        None => Err(ApiError::Internal),
    }
}

/// Runs a task mutation in one transaction as a member of `room`, then
/// publishes the resulting task to the room's members.
async fn mutate<F>(
    state: &State,
    caller: MemberId,
    room: RoomId,
    f: F,
) -> Result<Option<Task>, ApiError>
where
    F: FnOnce(&Connection, u64) -> R<Option<Task>> + Send + 'static,
{
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            let task = f(&tx, now_ms())?;
            if let Some(task) = &task {
                crate::require_task_fits(task)?;
                st.require_room_fits(&tx, &room)?;
            }
            let members = store::room_member_ids(&tx, &room)?;
            tx.commit()?;
            if let Some(task) = &task {
                st.hub.publish(
                    &members,
                    &Event::Task {
                        room,
                        task: task.clone(),
                    },
                );
            }
            Ok(task)
        })
        .await
}

/// Runs a project mutation in one transaction as a member of `room`, then
/// publishes the resulting project to the room's members.
async fn mutate_project<F>(state: &State, caller: MemberId, room: RoomId, f: F) -> Reply
where
    F: FnOnce(&Connection, u64) -> R<Project> + Send + 'static,
{
    let st = state.clone();
    let project = state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            let project = f(&tx, now_ms())?;
            st.require_room_fits(&tx, &room)?;
            let members = store::room_member_ids(&tx, &room)?;
            tx.commit()?;
            st.hub.publish(
                &members,
                &Event::Project {
                    room,
                    project: project.clone(),
                },
            );
            Ok(project)
        })
        .await?;
    Ok(Response::Project(project))
}

/// Deletes an empty project as a member of `room`, then tells the room's members.
async fn delete_project(state: &State, caller: MemberId, room: RoomId, project: u64) -> Reply {
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            store::delete_project(&tx, &room, project)?;
            let members = store::room_member_ids(&tx, &room)?;
            tx.commit()?;
            st.hub
                .publish(&members, &Event::ProjectDeleted { room, project });
            Ok(())
        })
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
    let st = state.clone();
    state
        .store
        .run(move |conn| {
            let tx = conn.transaction()?;
            store::require_member(&tx, &room, &caller)?;
            store::remove_member(&tx, &room, &caller)?;
            st.sessions.clear(&room, &caller);
            let others = store::room_member_ids(&tx, &room)?;
            let member = Member {
                id: caller,
                name: store::person_name(&tx, &caller)?,
                online: false,
            };
            tx.commit()?;
            st.hub.publish(&[caller], &Event::Left { room });
            st.hub.publish(&others, &Event::Member { room, member });
            st.hub.publish(
                &others,
                &Event::Sessions {
                    room,
                    member: caller,
                    sessions: Vec::new(),
                },
            );
            Ok(())
        })
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
