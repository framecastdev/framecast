# Appendices (Spec v4.1)

**Version:** 0.4.1
**Status:** Merged from Spec v4 with v0.4.1 patch updates
**Last Updated:** 2025-01-30

---

## Appendix A: Spec Schema (JSONB)

Complete TypeScript interface definitions for Framecast specification structures.

```typescript
interface Spec {
  title: string;

  global_prompt?: {
    positive?: string;
    negative?: string;
  };

  global_audio?: {
    music?: AudioRef;
    ambient?: AudioRef;
  };

  symbols?: Record<string, {
    prompt: string;
    negative?: string;
    reference_images?: Array<{
      id: string;       // AssetFile.id
      url: string;      // Signed URL (populated at read time)
      weight?: number;  // 0.0-1.0, default 1.0
    }>;
    voice?: {
      asset_id: string;   // AssetFile.id for voice sample
    };
  }>;

  transition_presets?: Record<string, TransitionDefinition>;

  scenes: Array<{
    id: string;
    prompt: string;
    duration: number;  // 1-30 seconds
    order?: number;
    audio?: SceneAudio;
  }>;

  transitions?: {
    default?: TransitionRef;
    [key: string]: TransitionRef;  // "scene1->scene2" format
  };

  timeline?: Array<TimelineElement>;
}

// === Audio Types ===

interface AudioRef {
  asset_id: string;       // AssetFile.id or SystemAsset.id
  volume?: number;        // 0.0-1.0, default 1.0
  fade_in?: number;       // seconds
  fade_out?: number;      // seconds
  loop?: boolean;         // default false
}

interface SceneAudio {
  ambient?: AudioRef;
  music?: AudioRef;
  sfx?: Array<{
    asset_id: string;
    trigger?: string;     // description of when to play (for AI timing)
    start?: number;       // explicit start time in seconds
    volume?: number;      // 0.0-1.0
  }>;
  dialogue?: Array<{
    speaker?: string;     // symbol key that has voice defined
    text: string;         // what is spoken (for TTS or reference)
    start?: number;       // start time in seconds
    volume?: number;      // 0.0-1.0
  }>;
}

// === Transition Types ===

type TransitionRef = string | TransitionDefinition;

interface TransitionDefinition {
  type: 'cut' | 'fade' | 'dissolve' | 'wipe' | 'blur' | 'zoom' | 'flash' | 'glitch' | 'custom';
  duration?: number;
  params?: Record<string, any>;
  effects?: Array<{
    name: string;
    from?: number;
    to?: number;
    easing?: string;
  }>;
  audio?: {
    asset_id?: string;    // AssetFile.id or SystemAsset.id for transition sound
    volume?: number;
  };
}

// === Timeline Types ===

type TimelineElement =
  | { scene: string }
  | { transition: TransitionRef }
  | { flashback: { scenes: string[]; treatment?: TreatmentDef } }
  | { montage: { scenes: string[]; pace?: string; transition?: TransitionRef } };

interface TreatmentDef {
  desaturate?: number;
  vignette?: boolean;
  grain?: number;
  blur?: number;
}
```

---

## Appendix B: WELCOME_SPEC

The complete Welcome to Framecast spec constant demonstrating typical spec structure and assets.

```typescript
const WELCOME_SPEC: Spec = {
  title: "Welcome to Framecast",

  global_prompt: {
    positive: "cinematic, first-person POV, warm lighting, celebratory atmosphere, high quality",
    negative: "blurry, distorted, low quality, dark, empty"
  },

  global_audio: {
    ambient: {
      asset_id: "asset_ambient_theater",
      volume: 0.2
    }
  },

  symbols: {
    crowd: {
      prompt: "enthusiastic diverse crowd of people clapping and cheering, warm smiles, colorful clothing",
      negative: "hostile, empty, sad"
    },
    stage: {
      prompt: "grand theater stage with red curtains, spotlights, elegant wooden floor",
      negative: "dirty, broken, abandoned"
    }
  },

  scenes: [
    {
      id: "entrance",
      prompt: "POV walking through backstage corridor towards bright stage lights, anticipation building, muffled sounds of @crowd ahead, dramatic shadows",
      duration: 5,
      audio: {
        sfx: [
          {
            asset_id: "asset_sfx_footsteps",
            trigger: "walking",
            volume: 0.6
          }
        ],
        music: {
          asset_id: "asset_music_anticipation",
          volume: 0.4,
          fade_in: 1.0
        }
      }
    },
    {
      id: "welcome",
      prompt: "POV stepping onto @stage, blinded momentarily by spotlights, then seeing @crowd rising to their feet, thunderous applause, confetti falling, warm embrace of celebration",
      duration: 5,
      audio: {
        sfx: [
          {
            asset_id: "asset_sfx_applause",
            start: 0.5,
            volume: 0.8
          },
          {
            asset_id: "asset_sfx_confetti",
            start: 2.0,
            volume: 0.4
          }
        ],
        music: {
          asset_id: "asset_music_triumphant",
          volume: 0.7,
          fade_in: 0.5
        }
      }
    }
  ],

  transitions: {
    default: "cut",
    "entrance->welcome": {
      type: "fade",
      duration: 0.5,
      audio: {
        asset_id: "asset_transition_swoosh",
        volume: 0.5
      }
    }
  },

  timeline: null
};
```

---

## Appendix C: Job Progress Schema (JSONB)

Schema for tracking job rendering progress across phases with scene-level detail and previews.

```typescript
interface JobProgress {
  phase: 'queued' | 'initializing' | 'generating' | 'stitching' | 'finalizing';
  percent: number;  // 0-100

  scenes_total?: number;
  scenes_completed?: number;
  current_scene?: string;

  message?: string;

  previews?: Array<{
    scene_id: string;
    url: string;
    generated_at: string;
  }>;
}
```

---

## Appendix D: Job Output Schema (JSONB)

Schema for final job output including video, clips, metadata, and resolution information.

```typescript
interface JobOutput {
  video_url: string;

  scene_clips?: Array<{
    scene_id: string;
    url: string;
    duration: number;
  }>;

  thumbnail_url?: string;

  duration: number;
  resolution: string;  // e.g., "1920x1080"
  format: string;      // e.g., "mp4"
  size_bytes: number;

  metadata?: {
    model_used: string;
    seed?: number;
    generation_time_ms: number;
  };
}
```

---

## Appendix E: Estimate Response Schema

Schema for job estimation responses providing cost and timing forecasts.

```typescript
interface EstimateResponse {
  estimated_duration_seconds: number;    // total video duration
  estimated_credits: number;             // credits that will be charged
  estimated_generation_time_seconds: number;  // wall clock time to generate

  scenes: Array<{
    id: string;
    duration: number;
    credits: number;
  }>;

  warnings?: Array<{
    message: string;
  }>;
}
```

---

## Appendix F: Deferred Features (Updated in v0.4.1)

The following features are explicitly deferred from v1. Features listed here may be re-evaluated for inclusion in future versions based on user feedback and platform maturity.

**Notable Changes in v0.4.1:**
- ~~Team Creation~~ Ã¢â‚¬â€ Now supported via `POST /v1/teams` (see Section 8.2)
- ~~Credit Refunds~~ Ã¢â‚¬â€ Now supported with Runway-style automatic refunds (see Section 12.6)
- **Ownership Transfer** Ã¢â‚¬â€ Remains deferred; manual support process continues in v1

### Deferred Features Table

| Feature | Description | Rationale |
|---------|-------------|-----------|
| SymbolTemplate | Team-level reusable symbols | Simplify v1; inline symbols sufficient |
| TransitionPreset (shared) | Team-level reusable transitions | Simplify v1; inline presets sufficient |
| ProjectVersion | Full edit history | job.spec_snapshot covers rendered versions |
| Job Priority | Queue priority levels | Single queue for v1 |
| Spec Templates | System-provided starter templates | WELCOME_SPEC covers onboarding |
| AssetFile orphan cleanup | Automatic cleanup of unreferenced assets | Explicit asset management for v1 |
| Free tier | Zero-cost access | Credits-based from start |
| TTS Integration | Text-to-speech for dialogue | Asset-based audio only for v1 |
| AI Music Generation | Procedural background music | Asset-based audio only for v1 |
| Voice Cloning | Clone voice from samples | Asset-based audio only for v1 |
| **Ownership Transfer** | Transfer team ownership to another member | Manual support process for v1 |

### Feature Status Summary

| Status | Count | Features |
|--------|-------|----------|
| Supported (v0.4.1) | 2 | Team Creation, Credit Refunds |
| Deferred | 11 | SymbolTemplate, TransitionPreset (shared), ProjectVersion, Job Priority, Spec Templates, AssetFile orphan cleanup, Free tier, TTS Integration, AI Music Generation, Voice Cloning, Ownership Transfer |

### Rationale for Deferral

These features are deferred to maintain focus on core functionality in v1:

- **Team-Level Resource Templates** (SymbolTemplate, TransitionPreset shared): Inline definitions provide sufficient capability; shared templates can be implemented once teams stabilize
- **History and Versioning** (ProjectVersion): Current `job.spec_snapshot` mechanism provides rendered version tracking; full edit history deferred for UI/UX simplification
- **Queue Management** (Job Priority): Single FIFO queue adequate for launch; priority queues deferred pending user demand and performance metrics
- **Content Generation** (TTS, AI Music, Voice Cloning): Asset-based model provides flexibility; procedural generation deferred to reduce complexity and costs
- **Free Access** (Free tier): Credits-based model from day one ensures fair resource allocation; free tier evaluation for future versions
- **Cleanup Automation** (AssetFile orphan cleanup): Explicit asset management required for v1 billing accuracy; automation deferred pending lifecycle policies
- **Ownership Transfer**: Manual support via customer success team sufficient for v1; automated transfer deferred pending user access policy maturity

---

*End of Appendices (Spec v4.1)*
