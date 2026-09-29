# Terminal proxy environment keeps the selected scheme

## Status

Implemented; pending review.

## Context

New local terminals receive `http_proxy`, `https_proxy`, and `all_proxy` from the
saved network settings. A first version rewrote `socks5://host:port` to
`http://host:port` for the first two variables so Windows PowerShell's
`WebRequest` could use a mixed listener. That assumption is not true of every
SOCKS endpoint.

## Evidence

curl uses a protocol-specific proxy variable ahead of `ALL_PROXY`
(https://curl.se/docs/manpage.html). An HTTP URL in `http_proxy` therefore
sends HTTP proxy requests to a SOCKS-only port, and the original scheme left
in `all_proxy` does not correct it. `System.Net.WebProxy` and PowerShell 7
`-Proxy` accept an HTTP proxy URL; they do not accept SOCKS.

## Decision

Write the validated URL, with its original scheme, into all three variables.
Add `PEBREL_HTTP_PROXY` only when that URL is itself HTTP, for both a custom
address and an enabled system proxy. The PowerShell startup script consumes
that marker and does not read `ssh_proxy_url`. Jump and command proxies stay
out of the environment because Rust already rejects them. Do not list
`PEBREL_HTTP_PROXY` in `WSLENV`.

## Rejected alternatives

- Rewriting SOCKS to HTTP on the same host and port. A mixed listener may
  accept both, but a SOCKS-only listener will not.
- Leaving the original scheme only in `all_proxy`. curl and git prefer
  `http_proxy` / `https_proxy` when those are set.
- Parsing `ssh_proxy_url` again in the PowerShell script. That path accepts
  forms the environment exporter already refuses, and it missed the system
  proxy that Rust had already selected.

## Consequences

A SOCKS setting reaches curl and git as SOCKS. PowerShell `Invoke-WebRequest`
follows it only when the saved address is an HTTP URL. An already open
terminal does not change.

## Validation

`terminal_proxy_assignments` and `apply_terminal_proxy_env` cover scheme
preservation, the HTTP marker, IPv6 brackets, credential bytes, and removal of
a stale marker. The Windows prompt script test requires `PEBREL_HTTP_PROXY`
and forbids reading the raw proxy settings.

## Supersedes

None.

## Revisit when

PowerShell can be given a SOCKS proxy without pretending the endpoint speaks
HTTP.
