# Schwitzer logo

Design brief, concept marks and the decision for the Schwitzer app icon. The
concept sources live in `concepts/<id>/mark.svg`. Rendered previews are not
committed.

## Name and positioning

Schwitzer: "Agents fischen sich Tasks raus und schwitzen". It is a task room
where agents pick tasks out of a shared queue. The mark should identify the app
beside editor and terminal, and be calm and tactile like the app itself
(DESIGN.md, "The Slab Rack").

## Criteria

1. Appropriate, distinctive, simple, judged in that order.
2. Readable at 16 px and in grayscale.
3. No text in the mark.
4. Flat colours, no gradients or glows.
5. Drawn on a 128 grid.

## Palette

All colours are DESIGN.md tokens.

| Colour | Token |
| --- | --- |
| `#142019` | text |
| `#2a3a30` | night-line |
| `#0a0f0c` | — (darker than night-ground) |
| `#3dcb5c` | night-primary / running |
| `#1a7f35` | ink |
| `#03200b` | night-on-primary |
| `#5aa6f5` | pickable |
| `#f0a630` | planning |
| `#e4eee6` | night-text |

## Concepts

**a-rack.** Three slabs with stage lamps: planning, pickable, and the pulled
one in running green. The top slab is pulled out of the rack, so an agent
fishes a task out. It is the product's own picture.

**b-tropfen.** A green sweat drop whose shine is the square stage lamp. It ties
to the name and has the clearest silhouette, but it sits close to generic water
and eco icons.

**c-schwitzender-slab.** A white slab with a lamp and two text lines on an
Action Green plate, with drops coming off it. It is humorous but busy below
32 px.

**d-rack-tropfen.** a-rack plus a sweat drop beside the pulled slab. It joins
product and name. At 16 px the drop disappears and a-rack remains.

## Decision

d-rack-tropfen on the dark plate. The shipping copy is
`crates/schwitzer-app/assets/logo/mark.svg`; `src/brand.rs` renders it as the
window icon. `schwitzer.ico` (embedded in `schwitzer.exe` by `build.rs`) and
`schwitzer.icns` (in `Schwitzer.app`, `deploy/package.sh`) beside it are
generated with `cargo run -p schwitzer-app --example icons`; rerun it after
changing the mark.
