# chronokeep-portal
Software primarily built to interface with RFID readers and other software.

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

| Environment variable | Meaning |
| --- | --- |
| `PORTAL_WEB_PORT` | Port for the web page (default `8484`; `0` or `off` turns it off) |
| `PORTAL_WEB_ADDRESS` | Address to listen on (default `0.0.0.0`, any address; `127.0.0.1` allows only this computer) |
