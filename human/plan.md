# pi0

let's design a personal intelligence workbench. I call this pi0 for now. It could collect laptop usage information and organise them, for user to analyse and have a better understanding of how to optimise work and life balance, or persue a smarter work style.

do make the plan clean, short and easy to understand.

## milestone 5, 20260705-1

### functionality requirements

- adaptive interval for screen capture and ocr. reduce power consumption, database size and cpu usage, enhance query speed and accuracy
- in settings - capture settings, "Screenshot interval" becomes two settings: active screenshot interval and idle screenshot interval
- active interval default 8s, idle default 48s
- in settings - capture settings, add a 'idle timeout', default 180s

### technical requirements

- extend rust addon to detect cursor/mouse/pointer movement delta events
- if have keystrokes/mouse movements in last 'idle timeout' window, we take it as 'active' and use active interval. otherwise use idle interval
- refine all time or timestamp related stuff, both in frontend, nodejs process and in rust addon, that events must also stamps timezone of the event. that is in db, local time (without timezone) + utc time + timezone name. 3 cols must present in the db, or we will run into issues when user moves across timezones
- agents and user-facing stuff should keep use local timezone, and make this interface behaviour explicit in all descriptions of MCP

## milestone 4, 20260704-1

### functionality requirements

- main window once shown, should always be shown in dock, but when hidden it must not show up in dock
- remove "Data folder" in Settings
- when launching, prompt to ask user to enter a password, which will be used as password for sqlite
- warn user this password can not be recovered once forget
- settings page does not require password
- user must be able to change this password in Settings, of course enter correct password first
- settings ordering: capture settings -> mcp server -> password reset
- macos permissions is not needed because we already have launch guard
- all settings wrapped inside a scrollable pane, and in the bottom a fixed row of Revert+Save button
- revert and save button must have proper disabled status
- mcp server instead of current "save" button, two buttons:
    - Copy Token: copy token generated to clipboard
    - Copy for Agents: copy mcp installation instruction for agents, for quick installation

### technical requirements

- ocr must be a queue (channel) to handle cases where cpu is too slow processing OCR for pictures
- we will migrate completely to sqlite
- filesystem still used, that is under pi0 data dir, a sqlite with WAL is used
- under pi0 data dir, no longer use date level. now pictures uses <dataDir>/<app>/<ts>-<monitor-id>.png and will be deleted after ocred. If the app crashes with PNGs still queued, the next capture start runs a sweep
- the sqlite will be password protected, auth using user's password. if non-present create new db
- mcp auth token will be stored inside the sqlite
- branch `backup-mcp-auth-secure-storage` has something you could learn from, but not just copy from it, think about our latest requirements

## milestone 3, 20260703-1

### functionality requirements

- main window should be purely a settings window, remove unnecessary widgets like status indicator and headline
- screenshots is now mandatory, cannot be switched off
- must be able to act as a mcp server, so other agents could use our processed information, and user can ad-hoc query and do analysis
- mcp server should contains interfaces: `['/apps', '/app-guidance', '/contexts']` to fetch contexts
- mcp server description must include designed usage to let agent know how to use these capabilities
- this mcp server design made it easy to iterate and provide updates to help agents have better analysis

### technical requirements

- ocr related stuff must be implemented in Rust side
- use <https://crates.io/crates/ocr-rs> and PP-OCRv6_small_rec.mnn to contextualise screenshots and delete picture afterwards
- ocr use CPU only. embed the required model files into our app bundle
- ocr must have texts their corrdinates on screen information (normalised to [0, 1] pair), cause it will be helpful for agent to judge the functionality and purpose of text proses
- mcp server must be implemented in nodejs process, to encourage faster dev-cycle and enhance dev-experience
- `/apps` must have timerange, designed to be called by agent first, to know what apps are used in timerange
- `/app-guidane` will provide the agent how to analyse the contexts, like it will tell the agent `Feishu/Lark` is a IM app and agent should focus on recent messages, contacts name, thus forming useful working context. It also tells agent what to ignore so extra abuntant texts could be ignored by agents
- `/contexts` must have pagination params, to let agent know it could read by parts

## milestone 2, 20260701-2

### functionality requirements

- should have a tray icon, after which clicked popup a float panel
- for the float panel, 3 rows
    - 1st row, after clicked, toggle main window show and hide
    - a toggle switch is provided to switch record on and off
    - 3rd row, exit whole application
- a guard modal dialog each time booting, must check for all permission before entering app

### technical requirements

## milestone 1, 20260701-1

### functionality requirements

- once launched, pi0 will act as a keylogger (see ../native-key-logger impl)
- once launched, pi0 will take snapshots every fixed interval AND when triggered by certain key-combinations
- pi0 must have settings to allow user set the snapshot interval
- for this milestone pi0 will just organise recorded texts and pictures using filesystem folders, and choose a proper organising manner, like `<DATA_DIR>/<LOCAL_DATE>/<APP_NAME>/`. not sure whether `APP_NAME` needs normalisation to fit in filesystem limitation
- for this milestone pi0 will allow user to see recorded text data using time-range picker

### technical requirements

- pi0's typescript part must leverage schema declaration and validation library to enhance correctness
- pi0 will be an electron desktop app, but will include a Rust native addon, which our electron app must link against. we will implement keylogging, screenshots, and data heavy crud for pi0
- pi0's electron node process will query data from rust addon, and talk to LLM on demand (thus use ai sdk)
