# gemu

Learning Rust by practising some concepts learned. We are implementing some communication primitives to be offered 
through a simple API. Note that this is not a reliable project to be used as an actual library, this is just something
to practice and learn Rust.

---

## Navigating

Since we are creating multiple communication primitives in here, each one will be in its own module. Maybe, someday I 
will create something that uses all the primitives available in here. So, the project follows the Cargo layout where
we are using multiple workspaces, inside each workspace is followed the structure defined by Cargo itself, currently
we have:

* `abro/`: This stands for *A*tomic *BRO*adcast, so the atomic broadcast primitive is contained inside this workspace, 
the implementation is backed by etcd.
* `mepa/`: This stands for *ME*ssage *PA*ssing, so this contains a simple TCP communication primitive.
