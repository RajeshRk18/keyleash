# Keyleash

An early Linux prototype of a policy-driven secret runtime for locally launched processes.

> [!WARNING]
> Keyleash is in early development. Do not use it with production secrets yet.

## The problem

Secrets often enter local workflows through `.env` files or shell environment variables. That makes them ambient. Every descendant process may inherit values it never asked for. Secrets can also end up in logs, terminal output, support bundles, and commits.

Coding agents make this risk harder to contain. They often run with the same environment and filesystem access as the developer shell. A prompt injection, compromised dependency, or mistaken tool call can make an agent print a key, write it into a file, add it to a commit, or send it over the network.

Keyleash is intended to keep managed secrets out of the process environment. It launches a process with a private capability channel. In the planned request flow, the process would ask for each secret by name and session policy would decide whether to allow it. This would narrow which managed secrets an agent or tool could obtain.

## Planned model

```text
named recipe
    |
    v
Keyleash launches a child process
    |
    v
private fd 3 channel
    |
    v
Get("DATABASE_URL")
    |
    v
session policy decides Allow or Deny
    |
    v
secret source resolves an allowed value
```

The channel is an unnamed Unix `SOCK_SEQPACKET` pair. Keyleash keeps one endpoint. The child receives the other endpoint as file descriptor 3. There is no global broker socket in the current architecture.

## Current status

The process-bound transport foundation is implemented.

- Linux-only Rust workspace
- Unnamed Unix `SOCK_SEQPACKET` channel
- Child endpoint inherited as fd 3
- `CLOEXEC` safe descriptor handoff
- One-packet send and receive
- EOF-driven channel lifecycle
- Integration tests for process and descriptor ownership
- Manual syscall-level verification with `strace`

The CLI, request protocol, policy engine, recipes, secret storage, audit records, and release packaging are not implemented yet.

The next milestone adds the first request flow with versioned frames and pure policy decisions.

## Intended security model

Keyleash aims to reduce these risks.

- Ambient secret exposure through shell environments
- Agent access to secrets that its session never requested
- Accidental inheritance by unrelated child processes
- Plaintext secret files in repositories
- Secret delivery without a policy decision

Keyleash does not protect against these threats.

- Root
- A deliberately malicious process running as the same user
- A prompt-injected child leaking a secret that policy allowed and delivered
- An authorized child copying a secret after receiving it
- Compromise of the underlying secret source

Closing the capability channel prevents future requests. It cannot revoke plaintext that a child already received.

## Roadmap

1. Versioned request and response frames
2. Pure allow and deny policy evaluation
3. Named recipe configuration
4. Encrypted local secret storage
5. Session deadlines and lifecycle hardening
6. Durable audit records
7. Release packaging and maintenance checks

## Development

Keyleash currently targets Linux. The workspace pins Rust 1.98.1.

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The process channel tests use a real child process. They verify fd 3 inheritance, packet exchange, child exit, and EOF behavior.

## License

The workspace declares `MIT OR Apache-2.0`. License texts will be added before the first release.
