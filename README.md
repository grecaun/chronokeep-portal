# chronokeep-portal
Software primarily built to interface with RFID readers and other software.

Chronokeep Portal runs on a small Linux computer (usually a Raspberry Pi) at a race. It
connects to Zebra RFID readers, turns the tags they see into reads, saves every read to a
local database, and uploads them to Chronokeep Remote APIs. It's controlled from Chronokeep
desktop, its own web page, or an optional screen and buttons on the portal itself.

## How it works

### Readers and reads
* Portal talks to Zebra readers over LLRP (port `5084` by default). Readers are saved in the
  database; those marked to auto connect are connected a minute after portal starts, and
  portal reconnects if a connection drops.
* Each reader sends tag reports. A chip seen several times within the **read window** becomes
  one read, keeping the sighting with the strongest signal. The window is in tenths of a
  second (default `20`, two seconds).
* Chips are recorded as decimal or hexadecimal (**chip type**, default `DEC`).
* Portal beeps for each chip it reads (if sound is on), but stays quiet for a chip it beeped
  for within the **beep interval** (seconds, default `60`, at most `600`).
* With `PORTAL_ZEBRA_SHIFT` set, antenna numbers are remapped to match the physical antenna
  layout of Chronokeep's own portal boxes.

### Saving reads
Readers hand their reads to a read saver thread, which writes them to the SQLite database so
readers never wait on the database. When a reader disconnects it saves anything it was still
holding. If a save fails, the reads are kept and saved on the next try (a failure while
portal is stopping is logged).

### Uploading
Reads are uploaded to Chronokeep Remote APIs (`https://remote.chronokeep.com/` or a self-hosted
one) using an API key.
* **Auto upload** (off by default) uploads every **upload interval** (seconds, default `5`), or
  as soon as 25 new reads are waiting, whichever comes first. Reads go up in batches of 100 and
  are marked uploaded once the API confirms them.
* A **manual upload** sends everything not yet uploaded, optionally only a time range or a
  single reader.

### Heartbeats
For each Remote API with heartbeat turned on, portal sends a heartbeat as soon as it starts
and then every minute: its name (or the API's device name), uptime, battery voltage and
whether it's charging, upload errors, reader counts, whether it's reading, and the chip type.

### Notifications
* **ntfy:** with ntfy turned on, portal posts to an ntfy topic when it starts and shuts down,
  when readers start or stop reading (or fail to start), and when the battery is low.
* **Remote API:** portal tells the Remote API when it's shutting down, and passes on
  notifications from the computer (UPS power changes, high temperature, low battery).

### Battery
On portals with a battery monitor (I²C bus 1), portal reads the battery voltage and shows the
level on the screen and in Chronokeep desktop. At 30% and again at 15% it warns connected
clients, ntfy and the Remote API.

### Ways to control portal
* **Chronokeep desktop** finds portals on the network automatically: it broadcasts on UDP
  port `4488` and each portal replies with its name, an id and its control port.
* **Control socket:** a TCP port (the first free one from `4488`) that takes JSON requests.
  Up to six clients can be connected at once, plus one more from the portal computer itself.
  Clients are told about changes (new reads, reader status, settings) as they happen.
* **Web page:** see [Web page](#web-page).
* **Screen and buttons** (Linux only): a 20×4 character screen (Adafruit or PCF8574T) with
  five buttons for starting and stopping readers, changing settings, uploading, setting the
  time, and restarting, updating or shutting down.

## Stopping, restarting, updating and shutting down
Every way of stopping portal (the control socket, the web page, the screen, `quit` from
portal-quit, or the service stopping) goes through the same steps, so no reads are lost:

1. The request is recorded and portal stops accepting new work. Shutdown, restart and update
   don't happen yet.
2. The shutdown notifications are queued, then heartbeats and auto upload are stopped.
3. Readers are disconnected, and portal waits (up to 15 seconds) for every thread to finish,
   including readers saving their last reads.
4. The read saver is stopped last and saves everything it's been given.
5. Settings, readers and APIs are written to the backup file.
6. The shutdown sound plays (once, however portal was stopped).
7. Only now does portal shut the computer down, restart the service, or run the update
   script, and it waits for that command to finish.

An update request from the control socket or web page is checked first (Linux, script set,
script exists); if it can't run, the request fails and portal keeps running.

## Data and backups
* Reads, readers, APIs and settings are kept in a SQLite database,
  `./chronokeep-portal.sqlite` by default (`PORTAL_DATABASE_PATH` to change it).
* Every time portal stops it writes `./portal_backup.json` with its settings, readers and
  APIs (not reads). If portal starts and the database doesn't exist, it restores from that
  file.

## Web page

The portal serves a web page for controlling it from any browser on the network, at
`http://<portal computer>:8484/`. It can change every setting Chronokeep desktop can (name,
chip type, read window, sound, voice, beep interval, screen type, uploads, ntfy), add, edit,
start and stop readers, manage remote APIs, show recent reads, clear reads, and update,
restart, stop or shut down the portal. The **Set time** button in the title bar shows how far
the portal's clock is from the device you're using, and sets it to that device's time (or a
time you choose) in one click. Setting the clock only works on Linux.

The page uses the portal's own control socket, so changes show up in any connected copy of
Chronokeep desktop right away.

## Environment variables
Portal also reads these from a `.env` file in its working directory.

| Environment variable | Meaning |
| --- | --- |
| `PORTAL_WEB_PORT` | Port for the web page (default `8484`; `0` or `off` turns it off) |
| `PORTAL_WEB_ADDRESS` | Address to listen on (default `0.0.0.0`, any address; `127.0.0.1` allows only this computer) |
| `PORTAL_DATABASE_PATH` | Database file (default `./chronokeep-portal.sqlite`) |
| `PORTAL_UPDATE_SCRIPT` | Script run to update portal; updating is unavailable without it |
| `PORTAL_SCREEN_BUS` | I²C bus of the screen; no screen is used without it |
| `PORTAL_UP_BUTTON`, `PORTAL_DOWN_BUTTON`, `PORTAL_LEFT_BUTTON`, `PORTAL_RIGHT_BUTTON`, `PORTAL_ENTER_BUTTON` | GPIO pins of the buttons |
| `PORTAL_ZEBRA_SHIFT` | Any value remaps Zebra antenna numbers for Chronokeep's portal boxes |

## Running
```
chronokeep-portal          start normally
chronokeep-portal --quick  connect to saved readers after 1 second instead of 60 (also -q)
```

## Installing
`install.sh` (Linux, run as a user with sudo, needs internet) installs portal to `/portal/`:
* downloads the latest release for the computer's architecture (x86_64, aarch64 or armv7)
  and chronokeep-portal-quit;
* writes `run.sh`, which sets the environment variables above and logs to `/portal/logs/`;
* sets up the `portal` systemd service, which starts at boot, restarts on failure, and runs
  portal-quit to stop portal cleanly, plus a `portal-quit` service that stops portal before
  the computer shuts down;
* lets portal run `date`, `hwclock`, `shutdown`, `reboot` and `systemctl` without a password
  (for setting the clock, shutting down and restarting).

`update.sh` installs the latest release and `uninstall.sh` removes portal. `install.sh -f`
recreates the scripts, service files and sudo settings.

## Building and testing
```
cargo build --release
cargo test
```
`scripts/cross-compile.sh` builds release versions for the supported Linux architectures.
