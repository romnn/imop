## IMOP

IMOP (_image optimizer_) is a webserver. Think of it as either a static fileserver like `nginx`, which can resize your images on the fly (and cache them), or a microservice that can resize any url you give it.

Implemented in rust and based on `tokio`, `warp` and it's static file handling implementation, all IO operations are asynchronous and suffiently fast (although there are no benchmarks yet). Images are resized with the `image` crate as blocking tokio tasks.

In short, here is what `imop` provides:

- stand alone static file server with support for resizing images, and async file compression
- warp filter for async compression based on the content type
- warp filter for image resizing
- warp handler for resizing any image on the internet

#### Usage a library

You can use

#### TODO

- restructure the current code in different files as a library
- add image resizing utility function
- add generic cache implementation (filesystem and in memory implementations)
