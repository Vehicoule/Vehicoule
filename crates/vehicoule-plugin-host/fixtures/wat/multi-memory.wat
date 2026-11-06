;; A plugin with two memories of 300 pages (18.75 MiB) each, written by hand because Rust's toolchain emits one
;; memory. Its `describe` traps: the host must stop it at instantiation, on the sum of its memories.
(module
  (memory (export "memory") 300)
  (memory $second 300)
  (func (export "vehicoule:plugin/plugin@0.1.0#describe") (result i32) unreachable)
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) unreachable)
)
