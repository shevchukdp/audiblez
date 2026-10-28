---
name: Ship With Proof
description: Enforce "build is not done" discipline - never claim a feature, fix, binary, or product works without actually running it and capturing evidence (exit code, window appears, HTTP 200, test pass). Use whenever about to say "done", "works", "ready", or "shipped" for anything that executes: a GUI app, CLI, server, script, library integration, or config.
metadata:
  opencode/autoinvoke: true
---

# Ship With Proof

A successful **build, compile, typecheck, or lint is NOT proof it works.** So
is declaring victory on an unrun artifact. The failure mode this prevents: you
compile a binary, it links, you announce "done ✅" — and the thing actually
crashes on launch (or does nothing) because you never ran it.

## Non-negotiable

Before ANY claim that something *works / runs / is done / is ready / is
shipped*, you must have **run the artifact and captured real evidence in this
conversation.** If you have not run it, you may only say *"it compiles"* — never
*"it works"*.

"Build succeeded" is a claim about the **compiler**. "It works" is a claim about
**runtime behavior**. They are different and only the second needs the second
kind of proof.

## Minimum evidence, by artifact type

Always run it; always paste the actual output (not a paraphrase).

- **GUI app**: launch the executable. Proof = the process stays alive past
  startup (window created) and/or a screenshot. A clean compile means nothing —
  GUIs die in windowing/graphics init all the time. Capture exit code and stderr.
- **CLI / executable**: run it with a real, non-trivial input. Proof = actual
  stdout + `exit=0`, and the output is *correct*, not merely present.
- **Server / API**: start it, then hit a real endpoint. Proof = real HTTP status
  + body. A "server started" log line is not proof the route works.
- **Library / SDK integration**: write and run the smallest program that calls
  the changed code path. Proof = it runs and asserts the expected result.
- **Script / config change**: execute the path that consumes it. Proof = the
  resulting behavior, shown.
- **Bug fix**: reproduce the bug first (see it fail), apply the fix, see it
  pass. Proof = before *and* after. Never mark a crash "fixed" without a clean
  run.

## Checklist before saying done

1. What is the runtime behavior the user actually cares about?
2. What single command demonstrates that behavior?
3. Run it. Paste the unedited output, exit code, and any logs.
4. Did it do the *right* thing — not just *something*? (No silent errors, no
   non-zero exit, no empty output, no timeout, no "compiled OK" standing in.)
5. Only then claim done. If you can't run it, SAY SO: "compiles, not yet run"
   is an honest and complete status.

## Red flags that mean you are skipping proof

- Saying "it builds, so it should work."
- Reporting a linker/compiler success as a feature success.
- Claiming a GUI, server, or integration works without launching it.
- Trusting a subagent's "done / verified" report without reproducing the key
  check yourself.
- Using a green build as the final line of a summary that says "working."

## Antipattern that prompted this skill

A Windows GUI was "delivered" on the strength of a passing `cargo build` and an
on-disk `.exe`. It was never launched. In fact it crashed immediately with
`0xC0000005` (a CRT startup null-deref) — invisible to the compiler. Building an
artifact and shipping it without running it ships bugs, not products.

**Build it. Run it. Show it. Then, and only then, ship it.**
