# Frachter

Frachter is a file transfer tool. 
It's not directly peer-to-peer, it takes a detour through your server.
The reason it's not p2p through for example WebRtc is, 
that when a device is connected to the Windows hotspot,
it can't talk to the host or vice-versa. That means
regular tools like [snapdrop.net](https://snapdrop.net) don't work
well.

## Setup

* Clone the repo
* Create a `config.toml` like this:

```toml
# where to bind the server
bind = "127.0.0.1:port"
# a secret for tokens set by this instance
jwt-secret = ""
# a secret token that you input on the webinterface
token = ""
```

* Compile/Run the server `cargo b -r` or `cargo r -r`
