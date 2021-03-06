JÖVE
====

Like [LÖVE](https://love2d.org/) but with JavaScript instead of Lua.

JÖVE is a framework for making 2D games in JavaScript.

**WORK IN PROGRESS**

build
=====

```
$ cargo build  # produces target/debug/jove
```

running
=======

```
$ jove [options ...] <script> [args ...]

# example: log garbage collector activity
$ jove --trace_gc --trace_gc_verbose script.js
```

Run `jove --help` to see a list of all options.

Script arguments are available through the global `args` array.

example
=======

```js
// run with: jove example.js
const WHITE = [1,1,1,1];
const RED = [1,0,0,1];

let d = 0;
let s = Number(args[0]) || 1.0;

function render() {
    const x = 350 + 200 * Math.sin(d);
    const y = 250 + 150 * Math.cos(d);
    d = (d + .005) % 360;
    clear(...WHITE);
    rectangle(...RED, x, y, 100 * s, 100 * s);
}
```
