# dumbsterfire

`dumbsterfire` is like a dumpster-fire but for dumb people that don't care enough for their inbox to be sorted or empty but still go on rampage and remove a lot of emails. It's a CLI tool to download emails from IMAP servers and apply labels based on filter rules—perfect for creating backups of your email accounts or organizing your archived emails.

## Installation

```bash
cargo install --git https://github.com/nichtsfrei/dumbsterfire.git
```

## Usage

The tool provides three main commands:

### 1. Download - Fetch emails from IMAP

```bash
dumbsterfire download \
  --host imap.example.com \
  --port 993 \
  --username your-email@example.com \
  --password your-password \
  --output-dir ./output
```

Or use environment variables:

```bash
export IMAP_HOST=imap.example.com
export IMAP_USER=your-email@example.com
export IMAP_PASS=your-password
export OUTPUT_DIR=./output

dumbsterfire download
```

After running download, emails are stored in `output/imap.example.com/` with subdirectories organized by sender, recipient, date, and subject. A `sha256sums` file is created for integrity verification.

### 2. Label - Apply labels to downloaded emails

First, create your label definitions. The default label directory is:

`$XDG_CONFIG_HOME/dumbsterfire/` or `~/.config/dumbsterfire/` or `/etc/dumbsterfire/`

```
labels/
├── labels.json           # Label metadata (titles, descriptions)
├── invoice/
│   └── rule.filter       # Filter rules for invoice emails
├── insurance/
│   └── rule.filter       # Filter rules for insurance emails
└── infrastructure/
    └── rule.filter       # Filter rules for infrastructure emails
```

Then run:

```bash
# Apply labels and list matched emails in label files
dumbsterfire label \
  --output-dir ./output \
  --label-dir ./labels

# Extract matched emails (attachments, body) when applying labels
dumbsterfire label \
  --output-dir ./output \
  --label-dir ./labels \
  --extract
```

### 3. Email - Extract a single email file

```bash
dumbsterfire email ./output/email.eml
```

This extracts the body and attachments from the `.eml` file into an `extracted/` subdirectory.

## Label DSL

Labels are defined using filter files that use a LISP-style DSL to match emails. Each label directory contains a `rule.filter` file.

### Basic Syntax

```lisp
; Simple rule - match if field contains value
(contains (subject) "invoice")

; Match if field equals value exactly
(is (from) "billing@example.com")

; Combine multiple conditions with AND
(and
  (contains subject "invoice")
  (is from "billing@example.com"))

; Match if any condition is true with OR
(or
  (contains subject "invoice")
  (contains subject "rechnung")
  (contains subject "billing"))

; Exclude matches with NOT
(not (contains subject "spam"))
```

### Supported Fields

| Field | Description |
|-------|-------------|
| `subject` | Email subject line |
| `from` | Sender email address |
| `to` | Recipient email address |
| `date` | Date the email was sent |
| `body` / `content` | Email body text |
| `path` | Full filesystem path to the email file |
| `attachment_names` | Names of attachments (in filter eval) |

### Multi-field and Multi-value

You can specify multiple fields and multiple values:

```lisp
; Search for "invoice" in subject OR body
(contains (subject body) "invoice")

; Search for multiple values in same field
(contains subject "invoice" "rechnung" "billing")
```

## Default Paths

The tool uses XDG Base Directory standards when available, with sensible fallbacks:

| Path Type | XDG-based location | Fallback location |
|-----------|-------------------|-------------------|
| **Labels** | `$XDG_CONFIG_HOME/dumbsterfire/` | `/etc/dumbsterfire/` |
| **Output (emails)** | `$XDG_DATA_HOME/dumbsterfire/emails/` | `/var/lib/dumbsterfire/emails/` |

On Linux/macOS without XDG variables set, defaults are:

- **Labels**: `~/.config/dumbsterfire/`
- **Output**: `~/.local/share/dumbsterfire/emails/`

You can override these defaults using environment variables or command-line arguments.

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `IMAP_HOST` | - | IMAP server hostname (required for download) |
| `IMAP_PORT` | `993` | IMAP port (usually 993 for IMAPS) |
| `IMAP_USER` | - | IMAP username/email (required for download) |
| `IMAP_PASS` | - | IMAP password (or use `--password-from-stdin`) |
| `OUTPUT_DIR` | XDG or fallback path | Directory for downloaded emails |
| `LABEL_DIR` | XDG or fallback path | Directory containing label definitions |
