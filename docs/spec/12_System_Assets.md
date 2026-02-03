# 13. System Assets

## 13.1 Overview

System assets are pre-loaded assets available to all users within the Framecast platform.
These assets represent commonly-used audio elements (sound effects, ambient sounds, music,
and transitions) that serve as a foundation for content creation.
Unlike user-created assets, system assets are:

- **Pre-loaded**: Automatically available without explicit user creation
- **Universal**: Accessible to all authenticated users
- **Immutable**: Cannot be modified or deleted by individual users
- **Cataloged**: Organized by category with standardized metadata

System assets enable users to quickly build polished content without requiring
proprietary licensing agreements or custom audio production.

## 13.2 System Asset Identification

### Entity ID Format

System assets use a predictable ID format in the database:

```
asset_{category}_{name}
```

**Examples:**

- `asset_sfx_whoosh`
- `asset_ambient_crowd`
- `asset_music_triumphant`
- `asset_transition_fade`

This is the canonical identifier used in:

- The `SystemAsset.id` field (primary key)
- Spec `asset_id` references (e.g., `AudioRef.asset_id`)
- API responses

### URN Format (Logical Reference)

For consistency with other URN schemes, system assets can also be referenced using URN format:

```
framecast:system:{category}:{asset_name}
```

**Examples:**

- `framecast:system:sfx:whoosh` â†’ resolves to `asset_sfx_whoosh`
- `framecast:system:ambient:crowd` â†’ resolves to `asset_ambient_crowd`

**Conversion Rules:**

```
URN to ID:  framecast:system:{cat}:{name} â†’ asset_{cat}_{name}
ID to URN:  asset_{cat}_{name} â†’ framecast:system:{cat}:{name}
```

**Note:** In spec JSONB, always use the entity ID format (`asset_sfx_whoosh`),
not the URN format. The URN format is for documentation and API discoverability.

## 13.3 System Asset Categories

The following categories organize system assets by functional purpose:

| Category | Description | Common Use Cases |
|----------|-------------|------------------|
| `sfx` | Sound effects | UI interactions, punctuation, comedic effects |
| `ambient` | Ambient/background sounds | Atmosphere, environment, immersion |
| `music` | Musical compositions | Mood setting, underscore, emotional support |
| `transition` | Transition sounds | Scene changes, sequencing, pacing |

## 13.4 System Asset Catalog

The system asset catalog has been EXPANDED in v0.4.0 to include a comprehensive collection of pre-recorded assets.

### Category: sfx (Sound Effects)

| Name | Description | Duration | Tags |
|------|-------------|----------|------|
| `whoosh` | Light, quick swoosh effect | 0.5s | movement, air, fast, transition |
| `whoosh_heavy` | Heavy, powerful swoosh effect | 0.8s | movement, air, powerful, impact |
| `impact` | Generic impact/collision sound | 0.3s | collision, hit, punch, effect |
| `impact_metal` | Metallic impact/clang | 0.4s | metal, collision, clang, industrial |
| `click` | Light UI click sound | 0.2s | ui, interactive, button, light |
| `pop` | Quick pop/burst effect | 0.25s | pop, burst, playful, light |
| `applause` | Full applause track | 3.2s | crowd, celebration, approval, audience |
| `applause_short` | Short applause burst | 1.1s | crowd, celebration, approval, quick |
| `footsteps` | Normal walking footsteps | 1.8s | human, movement, walking, ambient |
| `footsteps_running` | Running footsteps pattern | 1.2s | human, movement, running, fast |
| `confetti` | Celebratory confetti sound | 0.6s | celebration, festive, playful, effect |
| `door_open` | Door opening creak | 0.7s | interaction, environment, creak, object |
| `door_close` | Door closing/slam | 0.6s | interaction, environment, slam, object |
| `typing` | Keyboard typing sequence | 1.4s | human, interaction, office, activity |
| `notification` | Alert/notification ping | 0.3s | ui, alert, attention, digital |

### Category: ambient (Ambient/Background Sounds)

| Name | Description | Duration | Tags |
|------|-------------|----------|------|
| `crowd` | Calm crowd murmur | 5.0s | crowd, people, background, calm |
| `crowd_excited` | Excited crowd chatter | 4.8s | crowd, people, background, energetic |
| `nature` | Forest ambience | 6.0s | nature, outdoor, birds, forest |
| `nature_rain` | Rainfall ambience | 5.5s | nature, outdoor, water, weather |
| `city` | Urban street sounds | 5.2s | city, outdoor, urban, busy |
| `city_night` | Night city traffic | 5.0s | city, outdoor, urban, night |
| `office` | Office workplace sounds | 5.8s | office, indoor, workplace, ambient |
| `theater` | Theater/cinema ambience | 4.5s | indoor, entertainment, venue, ambient |
| `factory` | Industrial factory sounds | 6.2s | industrial, machinery, factory, ambient |
| `cafe` | Coffee shop ambience | 5.3s | indoor, social, cafe, ambient |
| `ocean` | Waves and ocean sounds | 5.7s | nature, water, outdoor, weather |

### Category: music (Musical Compositions)

| Name | Description | Duration | Tags |
|------|-------------|----------|------|
| `tense` | Tension-building composition | 15.0s | mood, tension, dramatic, orchestral |
| `tense_minimal` | Minimal tension underscore | 12.5s | mood, tension, minimal, subtle |
| `triumphant` | Victorious, uplifting music | 18.0s | mood, victory, uplifting, orchestral |
| `peaceful` | Calm, relaxing composition | 20.0s | mood, peaceful, calming, instrumental |
| `energetic` | High-energy, driving music | 14.0s | mood, energetic, driving, upbeat |
| `anticipation` | Building anticipation theme | 13.5s | mood, anticipation, building, dramatic |
| `corporate` | Professional, corporate underscore | 19.0s | mood, corporate, professional, neutral |
| `emotional` | Emotionally evocative piece | 16.5s | mood, emotional, poignant, orchestral |
| `playful` | Light, whimsical composition | 12.0s | mood, playful, whimsical, upbeat |
| `dramatic` | Intense dramatic theme | 17.0s | mood, dramatic, intense, orchestral |

### Category: transition (Transition Sounds)

| Name | Description | Duration | Tags |
|------|-------------|----------|------|
| `swoosh` | Clean musical swoosh | 0.6s | transition, movement, clean, effect |
| `fade` | Gentle fade effect | 1.0s | transition, fade, smooth, effect |
| `glitch` | Digital glitch effect | 0.4s | transition, digital, glitch, modern |
| `boom` | Deep impact boom | 0.8s | transition, impact, deep, powerful |
| `reverse` | Reversed effect sound | 0.7s | transition, reverse, effect, unusual |
| `rise` | Rising tension effect | 1.2s | transition, rise, building, effect |

## 13.5 System Assets in Spec

System assets are referenced in spec definitions using standard asset reference syntax.
Below is an example of how system assets appear in YAML:

```yaml
version: 1.0
id: framecast:user:projects:prj_abc123:assets:ast_001
name: "Demo Project with System Assets"
created_at: "2024-01-15T10:30:00Z"
updated_at: "2024-01-15T14:22:00Z"

media_elements:
  - element_id: elem_001
    type: audio
    asset_id: asset_sfx_whoosh
    start_time: 0.0
    duration: 0.5
    volume: 0.8

  - element_id: elem_002
    type: audio
    asset_id: asset_ambient_crowd
    start_time: 0.5
    duration: 5.0
    volume: 0.5
    loop: true

  - element_id: elem_003
    type: audio
    asset_id: asset_music_triumphant
    start_time: 5.5
    duration: 18.0
    volume: 0.7

  - element_id: elem_004
    type: audio
    asset_id: asset_transition_fade
    start_time: 23.5
    duration: 1.0
    volume: 0.9
```

## 13.6 System Asset Validation

System asset references must conform to the following validation rules:

### URN Format Validation

- System asset URNs MUST follow the format: `framecast:system:{category}:{asset_name}`
- `category` MUST be one of: `sfx`, `ambient`, `music`, `transition`
- `asset_name` MUST be a valid asset identifier from the system catalog
- Asset names are case-sensitive and use lowercase with underscores

### Reference Resolution

- System asset references are resolved at spec evaluation time
- If a referenced system asset does not exist in the catalog, the reference is invalid
- Invalid references MUST trigger a validation error with severity `ERROR`
- Implementations MAY cache system asset metadata for performance optimization

### Asset Usage Rules

- System assets MAY be referenced unlimited times within a single spec
- System assets MAY be combined with user-created assets in the same spec
- System assets are read-only and CANNOT be modified through spec operations
- System assets CANNOT be deleted by users through normal operations
- System assets are always available regardless of subscription tier

### Metadata Consistency

- System asset metadata (duration, tags, description) is immutable
- Updates to system asset catalog are propagated to all clients
- System asset versions are tracked separately from spec versions
- Specs referencing deprecated system assets remain valid but may emit deprecation warnings

### Playback Validation

- Duration values MUST be positive numbers (seconds)
- Volume values MUST be in range [0.0, 1.0]
- Loop flags MUST be boolean values
- Playback references to non-existent assets MUST fail gracefully with user-friendly error messages
