Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## \[4.0.0\] - unreleased

This release aims to further improve Pueue and to rectify some old design decisions.

### Removing internal channel communication

TLDR: Commands that start/stop/pause tasks now only return when the task is actually started/stopped/paused.
`kill`ing commands still takes a short while, as the process needs to be cleaned up properly.

---

Until recently, Pueue managed subprocess (task) states in a dedicated thread.
Client commands affecting subprocesses, such as `pueue start --immediate`, were relayed to this special thread via an `mpsc` channel for processing.

This setup caused short delays before the instructions were executed.
For instance, tasks would begin a few hundred milliseconds after the client received an `Ok` from the daemon, despite using the `--immediate` flag.
This behavior was unintuitive and often led to commands like `pueue add --immediate install_something && pueue send 0 'y\n'` failing, as the task had not started by the time `pueue send` was called.

The new state design resolves this issue by allowing Pueue to manipulate subprocess states directly within the client message handlers, eliminating any delays.

### New editing

Task editing was a bit tedious until recently.
One could only edit a single task at a time and you had to specify which properties you wanted to add.
Each property was then opened in a new `$EDITOR` session, which meant that users had to open and close editors up to four times to edit a single task.

After a lot of consideration, a new way of editing tasks has been designed that allows simple and convenient editing of multiple tasks at once.
For this, a temporary directory is created for every task to edit and a new file for every property, resulting in the following structure:

```
   📁 0/
   │ * command
   │ * label
   │ * path
   └ * priority
```

You can then just navigate the resulting file structure and edit the properties you want in the editor of your choice.

I'm aware that this might not be for everyone, so feedback is very much encouraged over [here](https://github.com/Nukesor/pueue/issues/553).

### Runtime invariants

TLDR: A new task state representation has been introduced, that's significantly cleaner and fixes some bugs.
However, it breaks compatibility with old states, so ensure there are no important tasks in your queue before updating. You'll also need to recreate groups.

---

Previously, various task-state related invariants were manually enforced during runtime. For example, a `Queued` task should not have a `start` or `enqueued_at` time set.
Turns out, doing this manually is highly error-prone, as it is difficult to account for every state transition and ensure everything is set or cleaned up correctly.

Fortunately, this issue can be addressed in a more elegant way in Rust using struct enums. This method enforces invariants via the type system at compile time.
Although the affected code became slightly more verbose (about 25% larger), it eliminated an entire class of bugs.
During this refactoring, I discovered at least two instances where I had forgotten to clear a variable, leading to inconsistent state.

Since the new structure differs significantly from the old one, it completely breaks backward compatibility.
Upon updating Pueue and restarting the daemon, the previous state will be wiped, resulting in a **clean slate**.

### Change

- **Breaking**: Refactor internal task state. Some task variables have been moved into the `TaskStatus` enum, which now enforces various invariants during compile time via the type system.
  Due to this, several subtle time related inconsistencies (task start/stop/enqueue times) have been fixed. [#556](https://github.com/Nukesor/pueue/pull/556) \
  **Important: This completely breaks backwards compatibility, including previous state.**
  **Important: The Pueue daemon needs to be restarted and the state will be wiped clean.**
- **Breaking**: Streamlined `pueue log` parameters to behave the same way as `start`, `pause` or `kill`. [#509](https://github.com/Nukesor/pueue/issues/509)
- **Breaking**: Remove the `--children` commandline flags, that have been deprecated and no longer serve any function since `v3.0.0`.
- Send log output to `stderr` instead of `stdout` [#562](https://github.com/Nukesor/pueue/issues/562).
- Change default log level from error to warning [#562](https://github.com/Nukesor/pueue/issues/562).
- Bumped MSRV to 1.70.
- **Breaking**: Redesigned task editing process [#553](https://github.com/Nukesor/pueue/issues/553).
  Pueue now allows editing all properties a task in one editor session. There're two modes to do so: `toml` and `files`.
- Revisited, fixed and cleaned up CLI help texts.
- Print most of Pueue's info/log messages to `stderr`. Only keep useful stuff like json and task log output on `stdout`.
- **Breaking** Uses `color-eyre` for a generic error type instead of `anyhow`, potentially breaking library users

### Add

- Add support for NetBSD.
- Add `--all` and `--group` to `pueue log`. [#509](https://github.com/Nukesor/pueue/issues/509)
- Add `--all` and `--group` to `pueue enqueue`. [#558](https://github.com/Nukesor/pueue/issues/558)
- Add `--all` and `--group` to `pueue stash`. [#558](https://github.com/Nukesor/pueue/issues/558)
- Add `pueue reset --groups [group_names]` to allow resetting individual groups. [#482](https://github.com/Nukesor/pueue/issues/482) \
  This also refactors the way resets are done internally, resulting in a cleaner code architecture.
- Ability to set the Unix socket permissions through the new `unix_socket_permissions` configuration option. [#544](https://github.com/Nukesor/pueue/pull/544)
- Add `command` filter to `pueue status`. [#524](https://github.com/Nukesor/pueue/issues/524) [#560](https://github.com/Nukesor/pueue/pull/560)
- Allow `pueue status` to order tasks by `enqueue_at`. [#554](https://github.com/Nukesor/pueue/issues/554)
- Added Windows service on Windows to allow a true daemon experience. [#344](https://github.com/Nukesor/pueue/issues/344) [#567](https://github.com/Nukesor/pueue/pull/567)
- Add `queued_count` and `stashed_count` to callback template variables. This allows users to fire callbacks when whole groups are finished. [#578](https://github.com/Nukesor/pueue/issues/578)
- Add new subcommand to set or unset environment variables for tasks. [#503](https://github.com/Nukesor/pueue/issues/503)
- Add `add --follow` flag that may be called in combination with `--immediate` [#592](https://github.com/Nukesor/pueue/issues/592)

### Fixed

- Fixed delay after sending process related commands from client. [#548](https://github.com/Nukesor/pueue/pull/548)
- Callback templating arguments were html escaped by accident. [#564](https://github.com/Nukesor/pueue/pull/564)
- Print incompatible version warning info as a log message instead of plain stdout input, which broke json outputs [#562](https://github.com/Nukesor/pueue/issues/562).
- Fixed `-d` daemon mode on Windows. [#344](https://github.com/Nukesor/pueue/issues/344)

## \[3.4.1\] - 2024-06-04

### Added

- Nushell autocompletion script [#527](https://github.com/Nukesor/pueue/pull/527)
- Add FreeBSD process helper to facilitate FreeBSD builds

### Changed

- Replace `chrono-english` by the `interim` drop-in replacement. [#534](https://github.com/Nukesor/pueue/issues/534)

## \[3.4.0\] - 2024-03-22

### Added

- Support modification of task priorities via `pueue edit --priority/-o` and `pueue restart --edit-priority/-o` [#449](https://github.com/Nukesor/pueue/issues/449).
- If no output directory is provided in `completions`, the generated file is printed to `stdout` [#489](https://github.com/Nukesor/pueue/issues/489).
- Allow setting the `parallel_tasks` value of groups to `0`. Setting this value allows unlimited tasks for that group [#500](https://github.com/Nukesor/pueue/issues/500).

### Fixed

- Include priority in `Task`s' `Debug` output [#493](https://github.com/Nukesor/pueue/issues/493)
- Made the daemon exit gracefully (exit code 0) on SIGINT and SIGTEM. [#504](https://github.com/Nukesor/pueue/issues/504)
- Fix reading of configuration files that lacks a `shared` section. [#505](https://github.com/Nukesor/pueue/issues/505)
- Respect the `-g` flag when using the `status` filter query. [#508](https://github.com/Nukesor/pueue/issues/508)

## \[3.3.3\] - 2024-01-04

### Fixed

- Bump `ring` from 0.16 to 0.17 to add riscv64 support [#484](https://github.com/Nukesor/pueue/issues/484).
- Fix that `add --priority` flag tried to get multiple arguments [#486](https://github.com/Nukesor/pueue/issues/486).

## \[3.3.2\] - 2023-11-28

### Fixed

- Fixed panic when calling parallel without arguments [#477](https://github.com/Nukesor/pueue/issues/477)
- Fixed wrong default location for `pueue_aliases.yml` [#480](https://github.com/Nukesor/pueue/issues/480)
- Fix typos

## \[3.3.1\] - 2023-10-27

### Fixed

- Daemonization doesn't work if pueued is not in $PATH [#299](https://github.com/Nukesor/pueue/issues/299)

## \[3.3.0\] - 2023-10-21

### Added

- Support the `PUEUE_CONFIG_PATH` environment variable in addition to the `--config` option. [#464](https://github.com/Nukesor/pueue/issues/464)

### Fixed

- Support parameter parsing for signal names with capslock (`SIGINT`) and short name (`INT`|`int`). [#455](https://github.com/Nukesor/pueue/issues/455)
- Better error messages for pid related I/O errors. [#466](https://github.com/Nukesor/pueue/issues/466)

### Changed

- QoL improvement: Don't pause groups if there're no queued tasks. [#452](https://github.com/Nukesor/pueue/issues/452)
  Auto-pausing of groups was only done to prevent the unwanted execution of other tasks, but this isn't necessary, if there're no queued tasks.

### Added

- `clear` and `cleanup` aliases for `clean` subcommand.

The two following features are very new and marked as "experimental" for the time being.
They might be reworked in a later release, since working with shells is always tricky and this definitely need more testing.

- Experimental: Allow configuration of the shell command that executes task commands. [#454](https://github.com/Nukesor/pueue/issues/454)
- Experimental: Allow injection of hard coded environment variables via config file. [#454](https://github.com/Nukesor/pueue/issues/454)

## \[3.2.0\] - 2023-06-13

### Added

- Add the `-j/--json` flag to `pueue group` to get a machine readable list of all current groups. [#430](https://github.com/Nukesor/pueue/issues/430)
- Add `pueued.plist` template to run pueue with launchd on MacOS. [#429](https://github.com/Nukesor/pueue/issues/429)
- Add query syntax documentation to `pueue status` [#438](https://github.com/Nukesor/pueue/issues/429)
- Add the `--priority/-o` flag to `pueue add` [#429](https://github.com/Nukesor/pueue/issues/427).
  This feature can be used to have easier control in which order tasks are executed.
  This was previously only possible via `pueue switch`.
- Add the `success` wait status. With this status, `pueue` will exit with `1` as soon as a single task fails. [#434](https://github.com/Nukesor/pueue/issues/434)

### Fix

- Fix broken bash autocompletion. Temporarily changes the name in the help texts to `pueue` and `pueued`. [#426](https://github.com/Nukesor/pueue/issues/426)
- Reword, extend and format most subcommand help texts.

### Change

- Don't fail on `follow` if a followed task exists but hasn't started yet. [#436](https://github.com/Nukesor/pueue/issues/436)
- Fail with a `1` exit code, when a followed task disappears or doesn't exist in the first place. [#436](https://github.com/Nukesor/pueue/issues/436)

## \[3.1.2\] - 2023-02-26

## Fixed

- Fixed changes to stdout not being printed after each I/O copy when using `pueue follow`. [#416](https://github.com/Nukesor/pueue/issues/416)

## \[3.1.1\] - 2023-02-12

## Fixed

- Fixed missing newlines after `status`, `log` and `follow` [#414](https://github.com/Nukesor/pueue/issues/414).

## \[3.1.0\] - 2023-02-08

### Added

- Allow to wait for specific task state when using `pueue wait` [#400](https://github.com/Nukesor/pueue/issues/400).

### Fixed

- Point to a new patched fork of `darwin-libproc`, as the original has been deleted.
  This fixes the development builds for pueue on Apple platforms.

## \[3.0.1\] - 2022-12-31

### Fixed

- Bump `command-group` to fix broken windows process handling [#402](https://github.com/Nukesor/pueue/issues/402)

## \[3.0.0\] - 2022-12-12

This release was planned to be a much smaller one, but you know how it's like.

A new major version is appropriate, as the process handling has been completely refactored.
Thanks to the work of [@mjpieters](https://github.com/mjpieters), Pueue now uses process groups to manage subprocesses, preventing detached processes by default!
This also closes a long standing issue and brings the support for MacOs on par with Linux!

v3.0.0 also adds the long-requested feature to add a query/filter logic for the `status` command and lots of other quality of life improvements.
The test coverage and development tooling has never been better, the project continues to improve!

### Breaking Changes

- Tasks are now started in a process group, and `pueue kill` will kill all processes in the group [#372](https://github.com/Nukesor/pueue/issues/372).
  The `--children` cli flag has been deprecated (signals go to the whole group, always).
  This brings pueue's task handling in line with how interactive shells handle jobs.
  As a side-effect it prevents detached processes and thereby covers the 90% usecase users usually expect.

### Changed

- `pueue log` output now includes the task label, if any. [#355](https://github.com/Nukesor/pueue/issues/355)
- Enable `pueue edit` to edit multiple properties in one go.

### Added

- _status querying_! `pueue status` now implements the first version of a simple query logic.
  The filtering/order/limit logic is also applied to the `--json` output.
  This allows you to:
  - `columns=id,status,path` select the exact columns you want to be shown.
  - `[column] [<|>|=|~] [value]` Apply various filters to columns.
    There's only a fix amount of operations on a small amount of columns available for now.
    If you need more filtering capabilities, please create an issue or a PR :).
  - `limit [last|first] 10` limit the results that'll be shown.
  - `order_by [column] [asc|desc]` order by certain columns.
  - For exact info on the syntax check the [syntax file](https://github.com/Nukesor/pueue/blob/main/client/query/syntax.pest).
    I still have to write detailed docs on how to use it.
- Show a hint when calling `pueue log` if the task output has been truncated. [#318](https://github.com/Nukesor/pueue/issues/318)
- Add `Settings.shared.alias_file`, which allows to set the location of the `pueue_aliases.yml` file.
- Added functionality to edit a task's label [#354](https://github.com/Nukesor/pueue/issues/354).
- Added the `created_at` and `enqueued_at` metadata fields on `Task` [#356](https://github.com/Nukesor/pueue/issues/356).
  They'll only be exposed when running `status --json` for now.

### Fixed

- Interpret the `$EDITOR` command, when editing a task's command/path, as a shell expression instead of an executable ([#336](https://github.com/Nukesor/pueue/issues/336)).
  This gives users more control over how their editor should be started.
- Don't show the version warning message between daemon and client, when using any `--json` flag.
- Fix some test failures in non-standard environments for NixOS test suite ([#346](https://github.com/Nukesor/pueue/issues/346)).
- The time in pueue's logs will now be in localtime instead of UTC [#385](https://github.com/Nukesor/pueue/issues/385).
- MacOs support has been brought on par with Linux.

### Misc

- Continuation of testing the `pueue` client, pushing the test coverage from ~70% to ~73%.
- A codecov.yml syntax error was corrected, which prevented Codecov from applying the repository-specific configuration.
- CI tests are now run using cargo nextest, for faster test execution, flaky test handling and better test output.
- The macos test suite is now the same as that for Linux, including the client and daemon test suites.

## \[2.1.0\] - 2022-07-21

### Added

- Use the new `--color` command-line switch to control when pueue will use colors in its output. Fixes [#311](https://github.com/Nukesor/pueue/issues/311) by [mjpieters](https://github.com/mjpieters).
  The default is `auto`, which means it'll enable colors when connected to a TTY.
  The other options are `never` and `always`.

### Fixed

- Only style the `group` header in status output when on a TTY ([#319](https://github.com/Nukesor/pueue/pull/319)) by [mjpieters](https://github.com/mjpieters).

### Changed

- Exit `pueue follow` when reading logs, as soon as the followed task is no longer active.
- Properly formatted debug output.
- Hide `Task.envs` and `AddMessage.envs` in debug output, as they were too verbose and contained possibly sensible information.

### Misc

- Enable CI linting on all platforms ([#323](https://github.com/Nukesor/pueue/pull/323)) by [mjpieters](https://github.com/mjpieters).
- Add CI caching ([#322](https://github.com/Nukesor/pueue/pull/322)) by [mjpieters](https://github.com/mjpieters).
- Fix missing toolchain bug in CI ([#321](https://github.com/Nukesor/pueue/pull/321)) by [mjpieters](https://github.com/mjpieters).
- Set up code-coverage in CI.
- Tests suite for `pueue` client, pushing the test coverage from ~53% to ~70%.

## \[2.0.4\] - 2022-06-05

### Fixed

- Return the correct path from `pueue_lib::settings::configuration_directories()`,
  when we get a path from `dirs::config_dir()` (was `/home/<user>/.config/pueue.yaml/`, is now again `/home/<user>/.config/pueue/`).
- Use the correct path to delete the PID file during shutdown.

## \[2.0.3\] - 2022-06-04

### Fixed

- Use the `dirs` crate for platform specific directory discovery. [#311](https://github.com/Nukesor/pueue/issues/311)
  The previous trivial implementation was error prone in some edge-cases.
  For instance, Pueue fell back to the shared directory, if the `$XDG_RUNTIME_DIR` couldn't be found.
  This resulted in a reocurrence of [#302](https://github.com/Nukesor/pueue/issues/302) in non-XDG environments.

  Furthermore, Pueue used the wrong directories for its configuration and cache on Apple and Windows platforms.
  This is now fixed.

  This change is a bit tricky:

  - It's a fix on one hand (correct directories for Apple & Windows + fix for [#311](https://github.com/Nukesor/pueue/issues/311)).
  - It's somewhat of a **breaking change** for Apple & Windows on the other hand?

  I still decided to make this a patch release, as the next major release is still in the pipeline and needs a lot of work.
  [#302](https://github.com/Nukesor/pueue/issues/302) will still show up in Apple/Windows environments, as there doesn't seem to be runtime directory equivalent for those platforms.

## \[2.0.2\] - 2022-03-22

### Added

- Better debug output for migration instructions from v1 to v2 [#298](https://github.com/Nukesor/pueue/issues/298).
- Better error output and error context for some filesystem related errors (continuation).
- Add a new option to specify the location of the `PID` file: `shared.pid_path` [#302](https://github.com/Nukesor/pueue/issues/302).

### Fixed

- Some options weren't properly passed onto the forked daemon instance, when starting `pueued` with the `-d` flag.
  - the `-vvv` flags
  - the `--profile` option.
- Autocompletion shell scripts. Their generation is now also tested to prevent future regressions.
- Move the `PID` file into the runtime directory to prevent rare startup issues after crashes + reboot. [#302](https://github.com/Nukesor/pueue/issues/298).
  This won't cause any problems for running clients/daemons, making this a backward compatible change.
- The `format-status` option now respects the order in which tasks piped back into pueue, as long as they're passed in list form [#301](https://github.com/Nukesor/pueue/issues/301).
  Tasks that're passed as a map will still be displayed in increasing order.

## \[2.0.1\] - 2022-03-12

### Added

- Better error output and error context for filesystem related errors [#239](https://github.com/Nukesor/pueue/issues/293).

### Fixed

- Commands no longer inherit environment variables from the daemon process by [drewkett](https://github.com/drewkett) [#297](https://github.com/Nukesor/pueue/pull/297).
  Previously, the daemon environment variables bled into the subprocesses.

## \[2.0.0\] - 2022-02-18

This release marks the second stable release of Pueue.

Shortly after releasing `v1.0.0` a few short-comings of some design decisions became apparent.
This release aims to remove all those short-comings or important missing features.
Some of those changes required breaking changes of both internal APIs and datastructures, as well as the CLI interfaces and the configuration file.
Since this project sticks to SemVer, this meant that a new major release was necessary.

Hopefully, this will be the last stable release for quite a while.
There are a few features planned that might introduce further breaking changes, but those will most likely need quite some time to implement (if we manage to implement them at all).

Anyhow, I'm quite pleased with the overall state of this release!
A lot of cool and convenient stuff has been added and quite a bit of internal logic has been streamlined and cleaned up.

Also a huge thanks to all contributors that helped working on this version!

### Added

- Shell auto-completion value hints for some arguments (zsh and fish only).
- Introduce the `rm` (remove), `re` (restart) and `fo` (follow) subcommand aliases [#245](https://github.com/Nukesor/pueue/issues/245).
- Allow to set the amount of parallel tasks at group creation by [Spyros Roum](https://github.com/SpyrosRoum) [#245](https://github.com/Nukesor/pueue/issues/249).
- When calling `pueue` without a subcommand, the `status` command will be called by default [#247](https://github.com/Nukesor/pueue/issues/247).
- Add the `--group` parameter to the `pueue clean` command [#248](https://github.com/Nukesor/pueue/issues/248).
- Add `output` for a task's log output as template parameters for callbacks [#269](https://github.com/Nukesor/issues/269).
- Add `--lines` parameter to `pueue follow` to only show specified number of lines from stdout before following [#270](https://github.com/Nukesor/pueue/issues/270).
- Notify the user if a task is added to a paused group [#265](https://github.com/Nukesor/pueue/issues/265).
- Notify the user that when killing whole groups, those groups are also paused [#265](https://github.com/Nukesor/pueue/issues/265).
- Implementation of configuration profiles [#244](https://github.com/Nukesor/pueue/issues/244).
  This supports multiple profiles in a single `pueue.yml`, which can be loaded via the `--profile/-p $name` flag.
- Added the `shared.runtime_directory` config variable for any runtime related files, such as sockets.
- `XDG_CONFIG_HOME` is respected for Pueue's config directory [#243](https://github.com/Nukesor/pueue/issues/243).
- `XDG_DATA_HOME` is used if the `pueue_directory` config isn't explicitly set [#243](https://github.com/Nukesor/pueue/issues/243).
- `XDG_RUNTIME_DIR` is used if the new `runtime_directory` config isn't explicitly set [#243](https://github.com/Nukesor/pueue/issues/243).
- The unix socket is now located in the `runtime_directory` by default [#243](https://github.com/Nukesor/pueue/issues/243).
- The `format-status` subcommand [#213](https://github.com/Nukesor/pueue/issues/213).
  This is a preliminary feature, which allows users to use external tools, such as `jq`, to filter Pueue's `state -j` output and pipe them back into `format-status` to display it.
  This feature will probably be removed once a proper internal filter logic has been added. \
  The simplest usage looks like this: `pueue status --json | jq -c '.tasks' | pueue format-status`
- Show currently active commands when calling `pueue wait`.

### Changed

- Improved memory footprint for reading partial logs.
- Always only show the last X lines of output when using `pueue log` without additional parameters.
- `pueue parallel` without arguments now also shows the groups with their current limit like `pueue group`. [#264](https://github.com/Nukesor/pueue/issues/264)
- Configuration files will no longer be changed programatically [#241](https://github.com/Nukesor/pueue/issues/241).
- Default values for all most configuration variables have been added [#241](https://github.com/Nukesor/pueue/issues/241).
- **Breaking changes:** `stderr` and `stdout` of Pueue's tasks are now combined into a single file.
  This means a few things.
  - One doesn't have to filter for stderr any longer.
  - All logs are now combined in a single chronologically correct log file.
  - One **can no longer** filter for stderr/stdout specific output.
- **Breaking changes:** The `group` subcommand now has `group add [-p $count] $name` and `group remove $name` subcommands.
  The old `group [-a,-p,-r]` flags have been removed.
- **Breaking changes:** The configuration for groups can no longer be done via configuration file.
  This means, that groups can only be edited, created or deleted via the commandline interface.
  The amount of parallel tasks will also be reset to `1` when upgrading.

### Removed

- No longer read `/etc/pueue/` configuration files.
  Pueue isn't designed as a system wide service, hence this doesn't make any sense to have system wide configuration files.
- If multiple configuration files are found, they're no longer merged together.
  Instead, only the first file will be used.

### Fixed

- Recover tasks from `Locked` state if editing fails [#267](https://github.com/Nukesor/pueue/issues/267)
- `pueue log` now behaves the same for local and remote logs.
  Remote logs previously showed more lines under some circumstances.
- panic due to rogue `.unwrap()` when filtering for a non-existing group in `pueue status`.

## \[1.0.6\] - 2022-01-05

#### Fixed

- The `--after` flag on add no longer accepted multiple parameters. This was due to a change in Clap's API in their bump from beta to full v3 release.

## \[1.0.5\] - 2022-01-02

### Changed

- Update to stable clap v3.0.

### Fix

- Panic instead of loop endlessly, if `task_log` directory disapears.

## \[1.0.4\] - 2021-11-12

### Fix

- Hard panic of the daemon, when one tries to switch a task with itself [#262](https://github.com/Nukesor/pueue/issues/262).

## \[1.0.3\] - 2021-09-15

### Fix

- The `default` group wasn't created, if the `pueue.yml` config file didn't contain it. [#242](https://github.com/Nukesor/pueue/issues/242).
  This lead to crashes and undefined behavior in the daemon and the client.
  This bug was introduced in `1.0.0` due to changes to the internal datastructures and several added features.
  It only popped up now, due to [#236](https://github.com/Nukesor/pueue/issues/236) being fixed, as the config is now being correctly used.
  This only affects users with quite old pueue configs or custom config files.

## \[1.0.2\] - 2021-09-12

### Feature

This feature wasn't supposed to be added to v1.0.2 and breaks semantic versioning.
I'm still getting used to this, sorry for any inconveniences.

- Add the `--working-directory` parameter to the `pueue add` command [#227](https://github.com/Nukesor/pueue/issues/227).

### Fix

- Settings weren't always read on daemon restart. [#236](https://github.com/Nukesor/pueue/issues/236).
  This bug was introduced in `1.0.0` due to large-scale refactorings and insufficient testing.

## \[1.0.1\] - 2021-08-20

### Fix

- Update to clap `v3.0.0-beta.4`. The upgrade from beta.2 to beta.4 introduced breaking changes, which lead to compiler errors when doing a `cargo install` without a `--locked`.
  A beta upgrade seems to be handled like a patch version in semantic versioning.
  This isn't a bug per se, but it leads to confusion when people forget the `--locked` flag during install.

## \[1.0.0\] - 2021-08-19

A lot of things happened during this release.
Even though quite a few new features were added, the main effort went into increasing stability and inter-version compatibility.

The goal of this release is to push the code quality, error handling, test coverage and stability to a level that justifies a v1.0 release. \
Since this project follows semantic versioning, this includes no breaking changes and backward compatibility on minor version upgrades. \
This also means that I'm quite certain that there are no critical bugs in the project and that all important and planned features have been implemented.

Unless some critical issues pop up, this can be seen as a finished version of the project!

**Disclaimer:** This project is mainly developed for Linux.
Windows and MacOS/Apple platforms are partially supported, but this is a community effort.
Thereby, v1.0 might be misleading for those. \
I hope you understand, that I cannot wait for someone to implement missing features for these platforms.
I want this project to move forward.

### Added

- `~` is respected in configuration paths by [dadav](https://github.com/dadav) for [#191](https://github.com/Nukesor/pueue/issues/191).
- Use `pueue kill --signal SigTerm` to send Unix signals directly to Pueue's processes. [#202](https://github.com/Nukesor/pueue/issues/202)
- Support for other `apple` platforms. New build artifacts for `ios-aarch64`.
- Option in config file to use the `--in-place` flag on `restart` by default.
- `--failed-in-group [group_name]` for `restart`. That way you can restart all failed tasks of a specific group [#211](https://github.com/Nukesor/pueue/issues/211)
- Options in config file to configure the time and datetime format in `pueue status` for [#212](https://github.com/Nukesor/pueue/issues/212).
- Add a worker pool representation for groups to Pueue [#218](https://github.com/Nukesor/pueue/issues/218).
  The task's group name and the pool's worker id for a given task are then injected into the environment variables of the subprocess.
  This allows users to map Pueue's internal group and worker logic to external resources:
  ```
  ./run_on_gpu_pool --gpu $PUEUE_WORKER_ID --pool $PUEUE_GROUP`
  ```
- The last lines of `stderr` and `stdout` are now available in the callback command. [#196](https://github.com/Nukesor/pueue/issues/196).
- Add `callback_log_lines` setting for the daemon, specifying the amount of lines returned to the callback. [#196](https://github.com/Nukesor/pueue/issues/196).
- Add a PID file to `$pueue_directory/pueue.pid`, which will be used to check whether there's an already running daemon.

### Changed

- Use the next available id instead of constantly increasing id's.
  This results in ids being reused, on `pueue clean` or `pueue remove` of the last tasks in a queue.
- Show the date in `pueue status` for the `start` and `end` fields, if the task didn't start today.
- Backward compatible protocol for stable version changes with `serde_cbor`.
- Detection of old daemon versions during client->daemon handshake.
- Overall better debug messages.
- Use tokio's async runtime and set a hardcoded limit of 4 worker threads, which is already more than enough.
- Add a debug message, when using `pueue wait` or `pueue wait -g some_group` and there're no tasks in the group.
- Stabilized internal daemon shutdown and restoration logic.
- Rename `Index` to `Id` in `pueue status` to free up screen space.
- Remove `Exitcode` column in `pueue status` and include exitcode into `Failed` status to free up screen space.
- You can no longer remove groups, if there are still tasks assigned to that group.
- A non-zero exit code will be returned, if no tasks were affected by an action.

### Datastructures

A whole lot of Pueue's internal datastructures have been refactored.
The main goal of this was to prevent impossible/invalid states wherever possible.

Overall, this resulted in sleaker und much better maintainable code. However, this broke backwards compatibility to pre-v1.0 at numerous places.

- Json structure of the `Task` struct changed significantly, as data depending on the current status has been moved into the `TaskStatus` enum.
- Many messages have been touched, as several new enums have been introduced and many fields have been removed.

### Fixed

- Handle very rare race-condition, where tasks with failed dependencies start anyway.
- `pueue log --json` now works again. [#186](https://github.com/Nukesor/pueue/issues/186)
  By default, only a few lines of output will be provided, but this can be configured via the `--full` and `--lines` option.
- Use crossbeam's mpsc channels, resulting in faster execution of user's instructions.
- Fix issue where the daemon was shutting down so fast, there wasn't enough time to respond the client that it's actually shutting down.

### Removed

- Removed the `enqueue` parameter from callback, as the callback is only run for finished tasks.

## \[0.12.2\] - 2021-04-20

### Fixed

- Remove task logs on `pueue remove`. [#187](https://github.com/Nukesor/pueue/issues/187)
- Improve Windows support by [oiatz](https://github.com/oiatz). [#114](https://github.com/Nukesor/pueue/issues/114)
- Fix empty output for empty groups when requesting specific group with `status -g $name`. [#190](https://github.com/Nukesor/pueue/issues/190)
- Fix missing output when explicitly requesting default group with `status -g default`. [#190](https://github.com/Nukesor/pueue/issues/190)

## \[0.12.1\] - 2021-03-12

### Fixed

- Dependant tasks didn't update the id of their dependencies, if a dependency's id was changed via `pueue switch` [#185](https://github.com/Nukesor/pueue/issues/185)

### Changed

- Show the status of the default group, if there are no tasks in the queue.

## \[0.12.0\] - 2021-02-10

**Info for all packagers:** \
In case you updated your packaging rules for the new layout in v0.11, those changes need to be reverted. \
The new repository layout with workspaces didn't work out that well.
Managing two crates in a single repository in combination with `cargo release` turned out to be quite annoying.

### Added

- `--all-failed` flag for `restart`.
  This will restart all tasks that didn't finish with a `Success` status. [#79](https://github.com/Nukesor/pueue/issues/79)
- New config option `client.dark_mode` by [Mephistophiles](https://github.com/Mephistophiles). [#178](https://github.com/Nukesor/pueue/issues/178)
  Default: `false`. Adds the ability to switch to dark colors instead of regular colors.

### Changed

- Rename/change some flags on the `restart` subcommand.
  1. Rename `--path` to `--edit-path`. The short flag stays the same (`p`).
  1. Rename the short flag for `--start-immediately` to `-k`.
- Dependency bump to pueue-lib `v0.12.1`

### Fixed

- `-s` flag overload on the `restart` command.
  `--start-immediately` and `--stashed` collided.
- Error on BSD due to inability to get username from system registry. [#173](https://github.com/Nukesor/pueue/issues/173)

## \[0.11.2\] - 2021-02-01

### Changed

- Readability of the `log` command has been further improved.
- Dependency bump to pueue-lib `v0.11.2`

## \[0.11.1\] - 2021-01-19

### Fixed

- Wrong version (`pueue-v0.11.0-alpha.0`) due to an error in the build process with the new project structure. [#169](https://github.com/Nukesor/pueue/issues/169)

## \[0.11.0\] - 2021-01-18

### Added

- Add the `--lines` flag to the `log` subcommand.
  This is used to only show the last X lines of each task's stdout and stderr.
- Add the `--full` flag to the `log` subcommand.
  This is used to show the whole logfile of each task's stdout and stderr.
- Add the `--successful-only` flag to the `clean` subcommand.
  This let's keep you all important logs of failed tasks, while freeing up some screen space.

### Changed

- If multiple tasks are selected, `log` now only shows the last few lines for each log.
  You can use the new `--full` option to get the old behavior.

## \[0.10.2\] - 2020-12-31

### Fixed

- It was possible to remove tasks with active dependants, i.e. tasks which have a dependency and didn't finish yet.
  This didn't lead to any crashes, but could lead to unwanted behavior, since the dependant tasks simply started due to the dependency no longer existing.
  It's however still possible to delete dependencies as long as their dependants are deleted as well.

## \[0.10.1\] - 2020-12-29

### Fixed

- panic, when using `pueue status` and only having tasks in non-default groups.

## \[0.10.0\] - 2020-12-29

This release adds a lot of breaking changes!
I tried to clean up, refactor and streamline as much code as possible.

`v0.10.0` aims to be the last release before hitting v1.0.0. \
From that point on I'll try to maintain backward compatibility for as long as possible (v2.0.0).\
Please read this changelog carefully.

### Changed

- Use TLS encryption for all TCP communication. [#52](https://github.com/Nukesor/pueue/issues/52)
- Updated Crossterm and thereby bump the required rust version to `1.48`.
- Extract the shared `secret` into a separate file. [#52](https://github.com/Nukesor/pueue/issues/52)
  This will allow users to publicly sync their config directory between machines.
- Change default secret length from 20 to 512 chars. [#52](https://github.com/Nukesor/pueue/issues/52)
- Lots of internal code cleanup/refactoring/restructuring.
- Exit client with non-zero exit code when getting a failure message from the daemon.
- The `group` list output has been properly styled.
- Use unix sockets by default on unix systems. [#165](https://github.com/Nukesor/pueue/issues/165)
- Any unix socket code or configuration stuff has been removed, when building for Windows.

### Added

- Add the `shared.host` configuration variable. [#52](https://github.com/Nukesor/pueue/issues/52)
  This finally allows to accept outside connections, but comes with some security implications.
- Create a self-signed ECDSA cert/key for TLS crypto with [rcgen](https://github.com/est31/rcgen). [#52](https://github.com/Nukesor/pueue/issues/52)
- Error messages have been improved in many places.
- `daemon.pause_all_on_failure` config, which actually pauses all groups as soon as a task fails.
- `daemon.pause_group_on_failure` config, which only pauses the group of the affected task instead of everything.
- Users can add some additional information to tasks with the `task add --label $LABEL` option, which will be displayed when calling `pueue status`. [#155](https://github.com/Nukesor/pueue/issues/155)
- `--escape` flag on the `add` subcommand, which takes all given Parameter strings and escapes special characters. [#158](https://github.com/Nukesor/pueue/issues/158)
- Remove `--task-ids` for `wait`. Now it's used the same way as start/kill/pause etc.
- Add an option `--print-task-id` to only return the task id on `add`. This allows for better scripting. [#151](https://github.com/Nukesor/pueue/issues/151)

### Removed

- Removed the `daemon.pause_on_failure` configuration variable in favor of the other two previously mentioned options.
- Removed the `--port` and `--unix-socket-path` cli flags on client in favor of the `--config` flag.
- Removed the `--port` flag on the daemon in favor of the `--config` flag.

### Fixed

- Properly pass `--config` CLI argument to daemonized `pueued` instance.
- The `--default` flag on the `kill` command has been removed, since this was the default anyway.
  That makes this command's behavior consistent with the `start` and `pause` command.
- Allow the old `kill [task_ids...]` behavior.
  You no longer need the `-t` flag to kill a tasks.
  This broke in one of the previous refactorings.

### Internal

- The default group is now an actual group.

## \[0.9.0\] - 2020-12-14

### Added

- The `wait` subcommand. This allows you to wait for all tasks in the default queue/ a specific group to finish. [#117](https://github.com/Nukesor/pueue/issues/117)
  On top of this, you can also specify specific tasks ids.
- New client configuration `show_expanded_aliases` (default: `false`).
  Determines whether the original input command or the expanded alias will be shown when calling `status`.
- New `--in-place` option for `restart`, which resets and reuses the existing task instead of creating a new one. [#147](https://github.com/Nukesor/pueue/issues/147)

### Changed

- Don't update the status of tasks with failed dependencies on paused queues.
  This allows to fix dependency chains without having to restart all tasks in combination with the `pause_on_failure` and the new `--in-place` restart option.

### Fixed

- `pause_on_failure` pauses the group of the failed tasks. Previously this always paused the default queue.
- Properly display version when using `-V`. (#143)
- Execute callbacks for tasks with failed dependencies.
- Execute callbacks for tasks that failed to spawn at all.
- Persist state changes when handling tasks that failed to spawn.
- Set proper start/end times for all tasks that failed in any way.

### Changed

- The original user command will be used when editing a task's command.
  As a result of this, aliases will be re-applied after editing a command.

## \[0.8.2\] - 2020-11-20

### Added

- Add `exit_code` parameter to callback hooks. (#138)
- Add a confirmation message when using `reset` with running tasks by [quebin31](https://github.com/quebin31). [#140](https://github.com/Nukesor/pueue/issues/140)

### Changed

- Update to beta branch of Clap v3. Mainly for better auto-completion scripts.

## \[0.8.1\] - 2020-10-27

### Added

- Add `start`, `end` and `enqueue` time parameters to callback hooks by [soruh](https://github.com/soruh).
- Config flag to truncate content in 'status'. (#123)

### Fixed

- ZSH completion script fix by [ahkrr](https://github.com/ahkrr).

## \[0.8.0\] - 2020-10-25

This version adds breaking changes:

- The configuration file structure has been changed. There's now a `shared` section.
- The configuration files have been moved to a dedicated `pueue` subdirectory.

### Added

- Unix socket support [#90](https://github.com/Nukesor/pueue/issues/)
- New option to specify a configuration file on startup for daemon and client.
- Warning messages for removing/killing tasks [#111](https://github.com/Nukesor/pueue/issues/111) by [Julian Kaindl](https://github.com/kaindljulian)
- Better message on `pueue group`, when there are no groups yet.
- Guide on how to connect to remote hosts via ssh port forwarding.

### Changed

- Move a lot of documentation from the README and FAQ into Github's wiki. The docs have been restructured at the same time.
- Never create a default config when starting the client. Only starting the daemon can do that.
- Better error messages when connecting with wrong secret.
- Windows: The configuration file will now also be placed in `%APPDATA%\Local\pueue`.

### Fixed

- Fixed panic, when killing and immediately removing a task. [#119](https://github.com/Nukesor/pueue/issues/119)
- Fixed broken non-responsive daemon, on panic in threads. [#119](https://github.com/Nukesor/pueue/issues/119)
- Don't allow empty commands on `add`.
- The client will never persist/write the configuration file. [#116](https://github.com/Nukesor/pueue/issues/116)
- The daemon will only persist configuration file on startup, if anything changes. [#116](https://github.com/Nukesor/pueue/issues/116)
- (Probably fixed) Malformed configuration file. [#116](https://github.com/Nukesor/pueue/issues/116)

## \[0.7.2\] - 2020-10-05

### Fixed

- Non-existing tasks were displayed as successfully removed. [#108](https://github.com/Nukesor/pueue/issues/108)
- Remove child process handling logic for MacOs, since the library simply doesn't support this.
- Remove unneeded `config` features and reduce compile time by ~10%. Contribution by [LovecraftianHorror](https://github.com/LovecraftianHorror) [#112](https://github.com/Nukesor/pueue/issues/112)
- Remove futures-timers, effectively reducing compile time by ~14%. [#112](https://github.com/Nukesor/pueue/issues/112)
- Update to comfy-table v1.1.0, reducing compile time by another ~10%. [#112](https://github.com/Nukesor/pueue/issues/112)

### Changed

- Linux process handling now always sends signals to its direct children, if the root process is a `sh -c` process.
  Previously, this behavior was somewhat ambiguous and inconsistent. [#109](https://github.com/Nukesor/pueue/issues/109)

### Added

- Update workflow to build arm binaries.

## \[0.7.0\] - 2020-07-23

### Added

- New `-e` and `-p` flags to edit tasks on restart.
  `-e` for `command`, `-p` for `path`.
  Both can be added at the same time.

### Changed

- Internal refactoring of the client code. Mostly structure.

### Fixed

- Improved CLI validation.
  Several subcommands accepted empty task id vectors, when they shouldn't.

## \[0.6.3\] - 2020-07-11

### Changed

- Don't do any code styling, if `stdout` is no tty.

## \[0.6.2\] - 2020-07-11

### Fixed

- Fix local `stderr` formatting for `log`.
- Fix missing sleep in local `follow` loop, resulting in single core 100% CPU usage.

## \[0.6.1\] - 2020-06-14

### Changed

- New default behavior for `follow`.
  Implemented by [JP-Ellis](https://github.com/JP-Ellis).
- Delete everything in Pueue's `task_logs` folder on `reset`.

## \[0.6.0\] - 2020-06-07

### Added

- `pueue_aliases.yml`, which allows some shell-like aliasing.
- `-c` flag for `kill` and `reset`.

## \[0.5.1\] - 2020-05-31

### Added

- `--children/-c` flag for `start` and `stop`.
  This sends the `SIGSTOP`/`SIGSTART` signal not only to the main process of a task, but also to direct children.
  This is, for instance, useful if you're starting tasks via a shell script.

### Fixed

- Fixed formatting bug in `pueue log`. Fixed by [sourcefrog](https://github.com/sourcefrog).

## \[0.5.0\] - 2020-05-15

### Added

- Groups! Tasks can now be assigned to a group.
  Each group acts as their own queue and each group has their own setting for parallel task execution.
  Groups can also be paused/resumed individually.
- Added `--group` flag for `status`. This will only print tasks of a specific group
- Add new flags `--default` to `kill`. With this flag only tasks in the default queue will be affected.
- Users can now specify a custom callback that'll be called whenever tasks finish.
- Environment variable capture. Tasks will now start with the variables of the environment `pueue add` is being called in.

### Changed

- `log` now also works on running and paused tasks. It thereby replaces some of `show`'s functionality.
- Rename `show` to `follow`. The `follow` is now only for actually following the output of a single command.
- `follow` (previously `show`) now also reads directly from disk, if `read_local_logs` is set to `true`.
- The `--all` flag now affects all groups AND the default queue for `kill`, `start` and `pause`.

## \[0.4.0\] - 2020-05-04

### Added

- Dependencies! This adds the `--after [ids]` option. Implemented by [tinou98](https://github.com/tinou98).
  Task with this option will only be started, if all specified dependencies successfully finish.
  Tasks with failed dependencies will fail as well.
- New state `FailedToStart`. Used if the process cannot be started.
- New state `DependencyFailed`. Used if any dependency of a task fails.
- New config option `read_local_logs`. Default: `true`
  We assume that the daemon and client run on the same machine by default.
  This removes the need to send logs via socket, since the client can directly read the log files.
  Set to `false` if you, for instance, use Pueue in combination with SSH port forwarding.

### Changed

- Pueue no longer stores log output in its backup files.
- Process log output is no longer permanently stored in memory. This significantly reduced RAM usage for large log outputs. Huge thanks for helping with this to [sourcefrog](https://github.com/sourcefrog)!
- Process log output is compressed in-memory on read from disk. This leads to reduced bandwidth and RAM usage.

## \[0.3.1\] - 2020-04-10

### Fixed

- Set `start` for processes. (Seems to have broken in 0.2.0)

## \[0.3.0\] - 2020-04-03

### Added

- `pause_on_failure` configuration flag. Set this to true to pause the daemon as soon as a task fails.
- Add `--stashed` flag to `restart`.
- Add `-p/--path` flag to allow editing of a stashed/queued task's path.
- Better network utilization for `pueue log`.

### Fixed

- Respect `Killed` tasks on `pueue clean`.
- Show `Killed` status in `pueue log`.
- Fix `pueue log` formatting.
- Show daemon status if no tasks exist.
- Better error messages when daemon isn't running.

## \[0.2.0\] - 2020-03-25

### Added

- New `--delay` flag, which delays enqueueing of a task. Can be used on `start` and `enqueue`. Implemented by [taylor1791](https://github.com/taylor1791).
- `--stashed` flag for `pueue add` to add a task in stashed mode. Implemented by [taylor1791](https://github.com/taylor1791).

### Changed

- Generating completion files moved away from build.rs to the new `pueue completions {shell} {output_dir}` subcommand.
  This seems to be the proper way to generate completion files with clap.
  There is a `build_completions.sh` script to build all completion files to the known location for your convenience.

### Fixed

- Fix `edit` command.
- Several wrong state restorations after restarting pueue.

## \[0.1.6\] - 2020-02-05

### Fixed

- \[BUG\] Fix wrong TCP receiving logic.
- Automatically create config directory.
- Fix and reword cli help texts.

## \[0.1.5\] - 2020-02-02

### Changed

- Basic Windows support. Huge thanks to [Lej77](https://github.com/Lej77) for implementing this!
- Integrate completion script build in `build.rs`.

## \[0.1.4\] - 2020-01-31

### Changed

- Dependency updates

## \[0.1.3\] - 2020-01-29

### Changed

- Change table design of `pueue status`.

## \[0.1.2\] - 2020-01-28

### Fixed

- Handle broken UTF8 in `show` with `-f` and `-e` flags.
- Allow restart of `Killed` processes.

## \[0.1.1\] - 2020-01-28

### Added

- Add --daemonize flag for daemon to daemonize pueued without using a service manager.
- Add `shutdown` subcommand for client for being able to manually kill the pueue daemon.

### Changed

- Replace prettytables-rs with comfy-table.
- Replace termion with crossterm.
