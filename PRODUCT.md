# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Stack

iced 0.14 (Rust)

## Users

Two developers (felix, tuco86) who each run omp coding agents on their own machine and subscription. Schwitzer sits narrow beside the editor and terminal all day; they glance at it to see what is being worked on, by whom, which plan is ready to pick up.

## Product Purpose

A task room for coding agents and their humans: someone opens a room without signing up and shares its key. Humans or agents note ideas and propose tasks; humans accept them; agents write plans; humans approve; agents build them one by one, each claiming the next free task. Success: everybody sees at a glance who works on what, which plans are finished and can be picked, and which agent sessions are live.

## Positioning

Agents and humans share one light queue with explicit human decisions (accept, approve, reject, replan, reopen, mark planned, mark done) and live visibility of each member's agent sessions, without Kanban ceremony and without anybody steering another person's agent.

## Operating Context

- Each member runs the desktop app; omp agents talk to it through the omp plugin the app installs in shared repos, which calls the app's local MCP server (`claim`, `submit_plan`, `finish`, ...).
- Agent sessions are omp sessions on the member's machine. A room holds projects, and every task belongs to one. Each member links local repositories to projects; a project's tasks show once this member has linked a repo to it, and active omp sessions whose working directory lies in a linked repo are shared with the room (title, project, runtime, tokens). Sessions outside those repositories never leave the machine.
- Picking an approved plan claims it for the member and copies a prompt with the claim marker for their own agent.
- Rooms live on a standalone `schwitzer-server`; a room key carries server address and secret.

## Capabilities and Constraints

- Stages: Idea (a rough sketch, written up into a proposal later), Proposed, Accepted (planning), Planned (plan ready), Ready (approved), In progress (Ready with a build claim, labelled "Building"), Done, Rejected. The list shows every claimed task, planning or building, under "In progress". Priority High/Normal/Low, no Kanban.
- Each task is a raised slab in the list that carries its own next-step button: Accept on a proposal, Approve on a finished plan, Pick on an approved one, Unclaim on your own claim. All other decisions (reject, replan, reopen, mark planned, mark done) stay in the open task. Right after an approval the slab keeps Approve, busy, for up to 6 s while the omp session that wrote the plan claims the build; only then does Pick appear.
- Settings is a page with a back button; a task opens in place in the list, and `+` opens a draft at the top of its group.
- Agents never decide: accept, reject, approve, replan, reopen, mark planned and mark done happen in the UI only.
- Desktop only (Linux, Windows, macOS); no web client.
- iced 0.14 exposes no accessibility tree; keyboard operation and contrast carry accessibility.

## Brand Commitments

- Name: Schwitzer ("Agents fischen sich Tasks raus und schwitzen").
- The user's direction for the look: take the construction of the classic (2003-2005) Steam client (stacked full-width panels in a recessed well, header bars, buttons and status labels inside the entries), not its colours; clean and modern; keep Figtree and the mint/green palette. A narrow window whose central element is the task list, every task a prominent, slightly larger, tactile raised element.

## Evidence on Hand

No real customers, screenshots or marketing assets. Demo content in captures is synthetic.

## Product Principles

1. The list is the product: who works on what must be readable in one glance.
2. Humans decide, agents work; decisions are explicit buttons, never side effects.
3. Nothing leaves the machine unless the member configured it (repos per room).
4. Narrow and calm: the app lives beside the work, not instead of it.
