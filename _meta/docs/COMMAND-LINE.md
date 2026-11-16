# Command line

The `legio` executable accepts three commands. A game ID is the UUID printed by
`legio list`.

| Command | Behavior |
| --- | --- |
| `legio list` | Prints one line per visible library game, `<id>`, a tab, then the name. Does not open a window. |
| `legio launch <id>` | Launches the game. If Legio is already running, the request is forwarded to that instance and its window is shown. Otherwise Legio starts and then launches the game. |
| `legio stop <id>` | Stops the running game through the running instance without showing its window. Fails if Legio is not running. |

Unknown or malformed arguments for these commands, including an invalid ID, are
rejected with a message on stderr and exit code 2. Desktop shortcuts keep using
`--launch-game=<id>`, which behaves like `launch`.

`list` reads the library database in read-only mode, so it works while Legio is
running and never migrates or modifies data. It exits with code 1 if the
database cannot be read.

## Windows console output

Release builds on Windows are GUI programs without a console. `legio list` attaches
to the console of the terminal that started it, but the shell does not wait for a
GUI program, so the prompt can return before the output appears. When output is
redirected, for example `legio list > games.txt`, it goes to the file normally.
