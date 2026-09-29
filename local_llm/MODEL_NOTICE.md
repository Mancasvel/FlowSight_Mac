# Bundled model notice

FlowSight 5 uses a locally quantized copy of **Qwen3.5-2B** by the Qwen team
and its visual projector. The original checkpoint is available at
https://huggingface.co/Qwen/Qwen3.5-2B, revision
`15852e8c16360a2fea060d615a32b45270f8a8fc`.

The original model is licensed under Apache License 2.0. A copy of its license
is bundled alongside this notice in `APACHE-2.0.md`. FlowSight converted the
checkpoint to GGUF, quantized the language weights to Q6_K and the visual
projector to Q8_0, and uses llama.cpp for local inference. These are format and
precision changes, **not** a fine-tune or an endorsement by the Qwen team.

| Asset | SHA-256 |
| --- | --- |
| `Qwen3.5-2B-Q6_K.gguf` | `381a869147e725e9e0087990f72ac5f3d5025aa3e4d0bc04b457fd7b30b6f7e4` |
| `mmproj-Qwen3.5-2B-Q8_0.gguf` | `351b26e2e94552a501d9b0d25455e34592d778def7e2e6d28cc9e7040f91c4ad` |

FlowSight's own source license is separate from this third-party model license.
