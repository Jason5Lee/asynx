# ASYNc eXecption

Simulate exception without `panic` in async Rust.

**DISCLAIMER**: This crate is just to implement my idea. It may not be a good practice.

Use in your project:

```toml
[dependencies]
asynx = "0.1"
```

Check [docs.rs docs](https://docs.rs/asynx/latest/asynx/) for usage.

You can use it in `no_std` environment by

```toml
[dependencies]
asynx = { version = "0.1", default-features = false }
```

which will disable `global` implementation.

Check [this blog](https://jason5lee.me/2022/03/11/rust-exception-async/) for the main idea.

**WARNING**: The sync implementation under `asynx::sync` has many unsafe code. Use it as your own risk.

## License

This project is distributed under the terms of both the MIT license and the Apache License (Version 2.0).
