## Codebase patterns/rules

Always run before committing:
  cargo cl        # strict clippy
  cargo t         # tests
  cargo fmt --check

Use ? for error propagation, never unwrap/expect.
Use .get() + match for indexing, never [i] directly.
All errors must be typed — no Box<dyn Error> in public APIs.
