# Bundled model notice

FlowSight 5 uses **Qwen3-VL-2B-Instruct** by the Qwen team as its single local
model. The GGUF weights and visual projector are the Qwen team's official
quantizations at https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct-GGUF,
revision `52d6c8ffea26cc873ac5ad116f8631268d7eb503`. The base checkpoint is
https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct.

The original model is licensed under Apache License 2.0. A copy of its license
is bundled alongside this notice in `APACHE-2.0.md`. FlowSight uses the
official Q4_K_M language GGUF and Q8_0 visual projector with llama.cpp for
local inference. Neither file is a FlowSight fine-tune or an endorsement by
the Qwen team.

| Asset | SHA-256 |
| --- | --- |
| `Qwen3VL-2B-Instruct-Q4_K_M.gguf` | `089d75c52f4b7ffc56ba998ffc50aae89fcafc755f9e7208aacca281dca6c2ae` |
| `mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf` | `f9a68fabba69c3b81e153367b2c7521030b0fa8bb0de400c9599c8e6725f9c82` |

FlowSight's own source license is separate from this third-party model license.
