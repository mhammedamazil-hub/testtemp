# Stonewild clean-room roadmap

Stonewild can grow toward a broad voxel-survival feature set, but “everything” is a multi-year product goal rather than a truthful single milestone. Each phase must remain independently playable and tested.

## Clean-room boundary

Allowed inputs:

- public observation of genre behavior;
- user-authored specifications;
- published algorithms and file-format standards;
- permissively licensed general-purpose libraries when their notices are retained.

Excluded inputs:

- Minecraft source or decompiled code;
- extracted textures, models, sounds, UI, maps, or data packs;
- copied names, branding, screen layouts, text, recipes, or progression tables;
- protocol compatibility presented as official Minecraft compatibility.

Mechanics, values, names, art direction, UI, recipes, creatures, structures, and progression will be independently designed and documented here.

## Phase 1 — engine foundation (implemented)

- deterministic terrain and bounded chunk streaming;
- block registry and procedural materials;
- collision, gravity, jumping, sprinting;
- ray-cast break/place interaction;
- save/load of player state and edits;
- face-culled meshing;
- GLES 3 renderer and touch/gamepad controls;
- debug APK workflow.

## Phase 2 — playable survival loop

- inventory screens and drag/drop touch UI;
- data-driven original recipes and crafting stations;
- health, stamina, hunger, fall damage, and respawn;
- item drops, pickups, tools, durability, and material tiers;
- time of day, sun/moon, weather, and configurable difficulty;
- biomes, improved caves, structures, fluids, and lighting propagation;
- original sound and generated/open-licensed texture packs.

## Phase 3 — creatures and simulation

- deterministic entity component system;
- pathfinding and spawn budgets;
- original passive, hostile, and utility creatures;
- farming, growth, breeding, status effects, and projectiles;
- circuits and automation using an independently designed signal system;
- explosions, portals, alternate realms, and boss-style encounters.

## Phase 4 — worlds and multiplayer

- versioned region storage, compression, migration, and recovery;
- local-network authoritative server;
- prediction, interpolation, interest management, and anti-cheat boundaries;
- permissions, chat, discovery, and private invite flow;
- Android lifecycle, background-save, tablet UI, accessibility, and controller remapping.

## Phase 5 — creator platform

- documented original mod API;
- data packs for blocks, items, recipes, biomes, structures, and creatures;
- shader/material packs with strict mobile budgets;
- world export/import and server administration;
- signed release builds, crash reporting, localization, and store-ready privacy disclosures.

## Definition of “playable”

A milestone is playable only when its build succeeds, automated tests pass, the APK installs, touch controls can enter a world, and a human can complete that milestone’s intended loop. Compilation alone is not called playable.
