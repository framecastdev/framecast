---
name: runpod-infra
description: ComfyUI and RunPod infrastructure. Use when working with Docker images, model downloads, network volumes, pod management, or video generation pipeline.
---

# RunPod & ComfyUI Infrastructure

## Architecture

Models stored on RunPod network volume. Custom Docker image symlinks model directories.

```
┌─────────────────────────────────────────────────────────────────┐
│                      RunPod Pod                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                  Custom Docker Image                         ││
│  │  /comfyui/models/checkpoints/ → /runpod-vol/models/ckpt/    ││
│  │  /comfyui/models/loras/       → /runpod-vol/models/loras/   ││
│  │  /comfyui/models/vae/         → /runpod-vol/models/vae/     ││
│  │  /comfyui/models/controlnet/  → /runpod-vol/models/cnet/    ││
│  └─────────────────────────────────────────────────────────────┘│
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              RunPod Network Volume                           ││
│  │  /runpod-vol/models/                                         ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Model Registry

Maintain models in `infra/runpod/models.yaml`:

```yaml
models:
  checkpoints:
    - name: sd_xl_base_1.0.safetensors
      url: https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0/resolve/main/sd_xl_base_1.0.safetensors
      sha256: 31e35c80fc4dd52dff28d28e78e2ac1ebfde34e83a6eb6d849f7f5f2e2e1eb9b
      size_gb: 6.94
  vae:
    - name: sdxl_vae.safetensors
      url: ...
  controlnet:
    - name: controlnet-canny-sdxl-1.0.safetensors
      url: ...
```

## Dockerfile Structure

```dockerfile
# infra/runpod/Dockerfile
FROM pytorch/pytorch:2.1.0-cuda12.1-cudnn8-runtime

RUN git clone https://github.com/comfyanonymous/ComfyUI.git /comfyui
WORKDIR /comfyui
RUN pip install -r requirements.txt

# Model directories (symlinked at runtime)
RUN mkdir -p /comfyui/models/{checkpoints,loras,vae,controlnet}

COPY scripts/entrypoint.sh /entrypoint.sh
COPY scripts/debug_server.py /debug_server.py

EXPOSE 8188  # ComfyUI
EXPOSE 8189  # Debug endpoints

ENTRYPOINT ["/entrypoint.sh"]
```

## Entrypoint Script

```bash
#!/bin/bash
# infra/runpod/scripts/entrypoint.sh
set -e

VOLUME_PATH="${RUNPOD_VOLUME_PATH:-/runpod-vol}"

# Create symlinks
ln -sfn "$VOLUME_PATH/models/ckpt" /comfyui/models/checkpoints
ln -sfn "$VOLUME_PATH/models/loras" /comfyui/models/loras
ln -sfn "$VOLUME_PATH/models/vae" /comfyui/models/vae
ln -sfn "$VOLUME_PATH/models/controlnet" /comfyui/models/controlnet

# Start debug server
python /debug_server.py &

# Start ComfyUI
exec python main.py --listen 0.0.0.0 --port 8188
```

## Debug Endpoints (Observability)

The pod MUST expose these endpoints for E2E debugging:

```
GET /debug/status     - Pod health, GPU info, current job
GET /debug/workflow   - Current workflow state, node progress
GET /debug/logs       - Recent ComfyUI logs
GET /debug/queue      - ComfyUI internal queue
GET /debug/memory     - GPU/system memory breakdown
GET /debug/artifacts  - Intermediate outputs for a job
```

## Just Targets

```bash
just runpod-build           # Build Docker image
just runpod-push            # Push to registry
just runpod-models-sync     # Download models to volume (run on pod)
just runpod-models-list     # List registered models
just runpod-models-verify   # Verify models on volume (run on pod)
just runpod-ssh "pod_id"    # SSH into pod
just runpod-logs "pod_id"   # Get pod logs
just runpod-restart         # Restart all pods
```

## Adding New Models

```bash
# 1. Add to models.yaml
vim infra/runpod/models.yaml

# 2. Sync to volume (run on a RunPod pod)
just runpod-models-sync

# 3. If new category/symlinks needed
just runpod-build
just runpod-push
just runpod-restart
```

## Directory Structure

```
infra/runpod/
├── Dockerfile
├── models.yaml           # Model registry (source of truth)
├── scripts/
│   ├── entrypoint.sh     # Container startup
│   ├── sync_models.py    # Download models
│   └── debug_server.py   # Observability endpoints
└── workflows/
    └── video_gen_sdxl.json
```
