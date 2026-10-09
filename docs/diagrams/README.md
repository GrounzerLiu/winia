# Diagrams

The Mermaid sources for [`architecture.md`](../architecture.md).

A `.mmd` file here renders as a diagram when you click it in the sidebar file tree
(with `dsh-mermaid-preview`); the same text appears as a fenced ` ```mermaid ` block
in `architecture.md`, which is what GitHub renders. The two copies are kept
byte-identical — if you edit one, edit the other, or regenerate it:

```
python - <<'EOF'
import pathlib
doc = pathlib.Path("docs/architecture.md")
order = ["layers", "frame", "state-handles", "overlay-host"]
lines = doc.read_text(encoding="utf-8").split("\n")
out, i, k = [], 0, 0
while i < len(lines):
    if lines[i].strip() == "```mermaid":
        j = i
        while lines[j].strip() != "```" or j == i:
            j += 1
        out.append("```mermaid")
        out += pathlib.Path(f"docs/diagrams/{order[k]}.mmd").read_text(encoding="utf-8").rstrip("\n").split("\n")
        out.append("```")
        k += 1
        i = j + 1
        continue
    out.append(lines[i]); i += 1
doc.write_text("\n".join(out), encoding="utf-8")
EOF
```

| file | what it draws |
| --- | --- |
| `layers.mmd` | the module layers, with the upward edges that exist on purpose |
| `frame.mmd` | one frame: event → recompose → materialize → layout → overlays → draw |
| `state-handles.mmd` | how a read subscribes and a write fans out |
| `overlay-host.mmd` | `PerWindow` and the overlays it hosts, each with its own Composer |

Keep the syntax conservative — `flowchart`, quoted labels, `<br/>` for line breaks,
`subgraph ID["title"]`. Nothing here needs anything newer, and the older subset is
what every Mermaid viewer renders.
