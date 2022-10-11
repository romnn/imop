## IMOP

IMOP (_image optimizer_) is a webserver. Think of it as either a static fileserver like `nginx`, which can resize your images on the fly (and cache them), or a microservice that can resize any url you give it.

Implemented in rust and based on `tokio`, `warp` and it's static file handling implementation, all IO operations are asynchronous and suffiently fast (although there are no benchmarks yet). Images are resized with the `image` crate as blocking tokio tasks.

In short, here is what `imop` provides:

- stand alone static file server
- support for resizing images
- support for async file compression
- flexible usage and highly extendable as a library
  - warp filter for async compression based on content type
  - warp filter for image resizing
  - warp handler for resizing any image downloaded over http

```bash
cargo run --example basic -- --images ./data --cache ./tmp
```

#### Usage a library

You can use

#### TODO

- done: can compress based on mime type and browser capabilites or specific algo, good api already
- done: can resize images based on optimizations

- make the image library generic trait as well
- implement the cache
- split into different examples
- implement the cli based production server
- do the image resize and encoding in a blocking pool

- restructure the current code in different files as a library
- add image resizing utility function
- add generic cache implementation (filesystem and in memory implementations)
- add examples to find a good api
- lint with clippy
- add tests
