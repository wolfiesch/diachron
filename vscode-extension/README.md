# Diachron - AI Code Provenance

See who (or what) wrote each line of code. Diachron tracks AI-generated code and shows inline blame for Claude Code, Codex, Aider, and more.

## Features

### Inline Blame on Hover

Hover over any line to see if it was written by an AI assistant:

- **Tool identification**: Claude Code, Codex, Aider, Cursor
- **Session tracking**: Link to the original AI session
- **Intent display**: See *why* the code was written
- **Confidence scoring**: HIGH, MEDIUM, LOW indicators

### Gutter Icons

Visual indicators in the editor gutter:
- 🟢 **Green**: High confidence AI-written code
- 🟡 **Yellow**: Medium confidence match
- ⚪ **Gray**: Low confidence / inferred

### Timeline View

Browse the complete history of AI changes in your project:
- Filter by file, tool, or time range
- Jump to specific changes
- Export for code reviews

## Requirements

- **Diachron daemon** must be running (`diachron daemon start`)
- **Diachron CLI** installed and initialized in your project (`diachron init`)

## Installation

1. Install from VS Code Marketplace (search "Diachron")
2. Or download `.vsix` from [releases](https://github.com/wolfiesch/diachron/releases)

## Commands

| Command | Description |
|---------|-------------|
| `Diachron: Blame Line` | Show AI provenance for current line |
| `Diachron: Show Timeline` | Open timeline view |
| `Diachron: Daemon Status` | Check daemon connectivity |

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `diachron.enabled` | `true` | Enable Diachron inline blame |
| `diachron.showGutterIcons` | `true` | Show gutter icons for AI lines |
| `diachron.hoverDelay` | `300` | Delay before showing hover (ms) |

## How It Works

1. **Diachron CLI** captures file changes via Claude Code hooks
2. **Diachron daemon** stores events in a local SQLite database
3. **This extension** queries the daemon via IPC to show inline blame

```
VS Code Extension ──────► Diachron Daemon
    │                          │
    └─── Unix socket IPC ──────┘
              │
    ~/.diachron/diachron.sock
```

## Privacy

All data stays local. Diachron never uploads your code or provenance data.

## Troubleshooting

**"Daemon not running"**
```bash
diachron daemon start
```

**No blame showing**
1. Ensure Diachron is initialized: `diachron init`
2. Check daemon status: `diachron daemon status`
3. Make sure you have captured events: `diachron timeline`

## Contributing

https://github.com/wolfiesch/diachron

## License

MIT
