# rjs
Native Javascript environment using Rust and Mozilla's SpiderMonkey JS engine.

## Why?
This project would like to be:
- A way to use Javascript to make Android games, using a thin WebGL/GLES binding.
- Javascript compatible with Node.js.
- An easy to use binding for Rust projects to embed a Javascript engine.
- A superhero.

This is going to take a lot of work, and there are things for everyone to do, so
peek into the Issues and look for a "help wanted" issue to get started!

## Why not?
- Why not use straight Rust?
  Javascript with hot module reloading has a faster edit-compile-run cycle.

- Why not V8?
  It's time for SpiderMonkey to find more uses outside of Firefox.

## Building from source

### Setting up dependencies

The single biggest dependency in this project is `rust-mozjs`, which requires
that you have the following installed:
* `cmake`
* `make`
* Python 2.7.x, which needs to be accessible as the `python` executable in your
    current `PATH`.

#### Arch Linux

```bash
# In the folder you want to clone into...
pacman -S base-devel python2 cmake
```

#### Windows (MSYS2)

1. Download and install [Python 2.7.x](https://www.python.org/downloads/).
    * Make sure that your `PATH` has your `python` executable pointing to this 2.7.x installation!
2. Install CMake:

```bash
pacman -S base-devel
pacman -S cmake
```

### Building

* After dependencies have been set up, you should only need [`cargo`](http://doc.crates.io/guide.html#working-on-an-existing-cargo-project) to build this project.
