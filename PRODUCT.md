# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Stack

iced 0.14 (Rust)

## Users

Two developers (felix, tuco86) who each run omp coding agents on their own machine and subscription. Schwitzer sits narrow beside the editor and terminal all day; they glance at it like a buddy list: what is being worked on, by whom, which plan is ready to pick up.

## Product Purpose

A task room for coding agents and their humans: someone opens a room without signing up and shares its key. Humans or agents note ideas and propose tasks; humans accept them; agents write plans; humans approve; agents build them one by one, each claiming the next free task. Success: everybody sees at a glance who works on what, which plans are finished and can be picked, and which agent sessions are live.

## Positioning

Agents and humans share one light queue with explicit human decisions (accept, approve, reject, replan, reopen) and live visibility of each member's agent sessions, without Kanban ceremony and without anybody steering another person's agent.

## Operating Context

- Each member runs the desktop app; agents talk to it through its local MCP server (`claim`, `submit_plan`, `finish`, ...).
- Agent sessions are omp sessions on the member's machine. Per room, each member lists local repositories in the settings; active omp sessions whose working directory lies in one of them are shared with the room (title, project, runtime, tokens). Sessions outside those repositories never leave the machine.
- Picking a finished plan claims it for the member and copies a prompt with the claim marker for their own agent.
- Rooms live on a standalone `schwitzer-server`; a room key carries server address and secret.

## Capabilities and Constraints

- Stages: Idea (a rough sketch, written up into a proposal later), Proposed, Accepted (planning), Planned (plan ready), Ready (approved), In progress (Ready with a build claim), Done, Rejected. The list shows every claimed task, planning or building, under "In progress". Priority High/Normal/Low, no Kanban.
- Agents never decide: accept, reject, approve, replan and reopen happen in the UI only.
- Desktop only (Linux, Windows, macOS); no web client.
- iced 0.14 exposes no accessibility tree; keyboard operation and contrast carry accessibility.

## Brand Commitments

- Name: Schwitzer ("Agents fischen sich Tasks raus und schwitzen").
- The user asked for a modern, fresh ICQ feel: a narrow buddy-list window whose central element is the task list.

## Evidence on Hand

No real customers, screenshots or marketing assets. Demo content in captures is synthetic.

## Product Principles

1. The list is the product: who works on what must be readable in one glance.
2. Humans decide, agents work; decisions are explicit buttons, never side effects.
3. Nothing leaves the machine unless the member configured it (repos per room).
4. Narrow and calm: the app lives beside the work, not instead of it.
