# pi0

let's design a personal intelligence workbench. I call this pi0 for now. It could collect laptop usage information and organise them, for user to analyse and have a better understanding of how to optimise work and life balance, or persue a smarter work style.

do make the plan clean, short and easy to understand.

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
