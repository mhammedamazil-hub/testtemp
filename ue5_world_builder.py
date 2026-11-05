"""
Project Neon-Underground — Unreal Engine 5 World Builder.

Editor automation script. Run inside the Unreal Editor Python console:

    exec(open("ue5_world_builder.py").read())

or via the UE5 commandlet:

    UE5Editor -run=pythonscript -script=ue5_world_builder.py

It requires the native `unreal` module, which only exists inside an editor
instance. The script is dependency-free otherwise.

Pipeline stages:
  1. Ingestion subsystem — bulk import .glb/.fbx meshes, .wav/.mp3 audio,
     and .png textures into /Game/Geometry/, /Game/Audio/, /Game/Textures/.
  2. Texture override layer — TF_Nearest (point filtering) + low-bandwidth
     compression on every imported texture (retro PSX aesthetic).
  3. Procedural map generator — parses a 10x10 integer city-block matrix
     and spawns modular road tiles, skyscrapers, streetlights and the
     player vehicle spawner at precise global 3D vector coordinates via
     EditorActorSubsystem.spawn_actor_from_asset().
"""

import os

try:
    import unreal
except ImportError as exc:
    raise SystemExit(
        "ue5_world_builder.py must run inside the Unreal Editor "
        f"(unreal module unavailable: {exc})"
    )


# --------------------------------------------------------------------------- #
# Path / folder setup
# --------------------------------------------------------------------------- #

SOURCE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")
RENDER_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "renders")
AUDIO_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "audio")
TEXTURE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "textures")

GEOM_FOLDER = "/Game/Geometry"
TEXTURE_FOLDER = "/Game/Textures"
AUDIO_FOLDER = "/Game/Audio"
LEVEL_FOLDER = "/Game/Levels"

TILE_SIZE_UU = 1000.0  # 10 m tile @ 100 UU/m


def _ensure_folder(path):
    unreal.EditorAssetLibrary.make_directory(path)


def _import_path_for(source_file, folder):
    name = os.path.splitext(os.path.basename(source_file))[0]
    return f"{folder}/{name}"


# --------------------------------------------------------------------------- #
# 1. Ingestion subsystem
# --------------------------------------------------------------------------- #

def import_mesh_assets():
    """Bulk-import .glb / .fbx meshes into /Game/Geometry/."""
    _ensure_folder(GEOM_FOLDER)
    asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
    tasks = []
    for fname in sorted(os.listdir(SOURCE_DIR)):
        if not fname.lower().endswith((".glb", ".fbx")):
            continue
        src = os.path.join(SOURCE_DIR, fname)
        dest = _import_path_for(src, GEOM_FOLDER)
        task = unreal.AssetImportTask()
        task.set_editor_property("filename", src)
        task.set_editor_property("destination_path", dest)
        task.set_editor_property("destination_name", os.path.splitext(fname)[0])
        task.set_editor_property("replace_existing", True)
        task.set_editor_property("automated", True)
        task.set_editor_property("save", True)
        # glTF/fbx import options
        opts = unreal.AutodeskFBXImportOptions() if fname.endswith(".fbx") else None
        if opts is not None:
            opts.set_editor_property("import_type", unreal.FBXImportType.FBXIT_SKELETAL_MESH)
            task.set_editor_property("options", opts)
        tasks.append(task)

    if tasks:
        asset_tools.import_asset_tasks(tasks)
        unreal.log(f"[ue5_world_builder] imported {len(tasks)} mesh assets -> {GEOM_FOLDER}")
    else:
        unreal.log_warning("[ue5_world_builder] no mesh assets found in assets/")
    return tasks


def import_audio_assets():
    """Bulk-import .wav / .mp3 into /Game/Audio/."""
    _ensure_folder(AUDIO_FOLDER)
    asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
    tasks = []
    for fname in sorted(os.listdir(AUDIO_DIR)):
        if not fname.lower().endswith((".wav", ".mp3", ".ogg")):
            continue
        src = os.path.join(AUDIO_DIR, fname)
        task = unreal.AssetImportTask()
        task.set_editor_property("filename", src)
        task.set_editor_property("destination_path", AUDIO_FOLDER)
        task.set_editor_property("destination_name", os.path.splitext(fname)[0])
        task.set_editor_property("replace_existing", True)
        task.set_editor_property("automated", True)
        task.set_editor_property("save", True)
        tasks.append(task)

    if tasks:
        asset_tools.import_asset_tasks(tasks)
        unreal.log(f"[ue5_world_builder] imported {len(tasks)} audio assets -> {AUDIO_FOLDER}")
    return tasks


# --------------------------------------------------------------------------- #
# 2. Texture override layer
# --------------------------------------------------------------------------- #

def _override_texture(asset_path):
    """Force nearest-neighbor filtering + low-bandwidth compression."""
    tex = unreal.EditorAssetLibrary.load_asset(asset_path)
    if tex is None or not isinstance(tex, unreal.Texture2D):
        return False
    tex.set_editor_property("compression_settings",
                            unreal.TextureCompressionSettings.TC_Default)
    tex.set_editor_property("filter", unreal.TextureFilter.TF_Nearest)
    tex.set_editor_property("mip_gen_settings",
                            unreal.TextureMipGenSettings.TMG_NoMipmaps)
    tex.set_editor_property("srgb", True)
    unreal.EditorAssetLibrary.save_asset(asset_path)
    return True


def override_all_textures():
    """Apply retro filter/compression settings to every imported texture."""
    _ensure_folder(TEXTURE_FOLDER)
    # First import any loose pngs from textures/ so the override has targets.
    import_texture_assets()
    assets = unreal.EditorAssetLibrary.list_assets(TEXTURE_FOLDER, recursive=True)
    count = 0
    for asset_path in assets:
        if _override_texture(asset_path):
            count += 1
    unreal.log(f"[ue5_world_builder] texture overrides applied: {count}")
    return count


def import_texture_assets():
    _ensure_folder(TEXTURE_FOLDER)
    asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
    tasks = []
    for fname in sorted(os.listdir(TEXTURE_DIR)):
        if not fname.lower().endswith(".png"):
            continue
        src = os.path.join(TEXTURE_DIR, fname)
        task = unreal.AssetImportTask()
        task.set_editor_property("filename", src)
        task.set_editor_property("destination_path", TEXTURE_FOLDER)
        task.set_editor_property("destination_name", os.path.splitext(fname)[0])
        task.set_editor_property("replace_existing", True)
        task.set_editor_property("automated", True)
        task.set_editor_property("save", True)
        tasks.append(task)
    if tasks:
        asset_tools.import_asset_tasks(tasks)
        unreal.log(f"[ue5_world_builder] imported {len(tasks)} textures -> {TEXTURE_FOLDER}")
    return tasks


# --------------------------------------------------------------------------- #
# 3. Procedural map generator
# --------------------------------------------------------------------------- #

# Tile key -> imported asset path in /Game/Geometry/
TILE_ASSET_MAP = {
    1: f"{GEOM_FOLDER}/Tile_Straight",
    2: f"{GEOM_FOLDER}/Tile_Intersection",
    3: f"{GEOM_FOLDER}/Tile_Building_A",
    4: f"{GEOM_FOLDER}/Tile_Prop_Set",
}

# 10x10 city block matrix.
# 0 = empty, 1 = straight road, 2 = intersection, 3 = skyscraper facade,
# 4 = prop set, 9 = player vehicle spawner
CITY_MATRIX = [
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 3, 4, 1, 2, 1, 3, 4, 3, 1],
    [1, 4, 0, 1, 1, 1, 0, 4, 0, 1],
    [1, 3, 0, 1, 2, 1, 0, 3, 0, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 0, 0, 1, 2, 1, 0, 0, 0, 1],
    [1, 4, 0, 1, 1, 1, 3, 0, 4, 1],
    [1, 3, 0, 1, 2, 1, 4, 0, 3, 1],
    [1, 0, 0, 1, 1, 1, 0, 0, 0, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
]


def _load_asset(asset_path):
    a = unreal.EditorAssetLibrary.load_asset(asset_path)
    if a is None:
        unreal.log_warning(f"[ue5_world_builder] asset not found: {asset_path}")
    return a


def spawn_procedural_map(matrix=None, start_location=unreal.Vector(0, 0, 0)):
    """
    Parse a 2D integer matrix and snap modular assets to their global
    3D vector coordinates using EditorActorSubsystem.spawn_actor_from_asset().
    """
    matrix = matrix or CITY_MATRIX
    rows = len(matrix)
    cols = len(matrix[0])
    actor_subsys = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)

    # ensure a level exists to spawn into
    level_subsys = unreal.get_editor_subsystem(unreal.EditorLevelSubsystem)
    level_subsys.new_level(LEVEL_FOLDER + "/Neon_Underground_City")

    spawned = 0
    for r in range(rows):
        for c in range(cols):
            key = matrix[r][c]
            if key == 0:
                continue
            if key == 9:
                asset_path = f"{GEOM_FOLDER}/SM_Interceptor"
                actor_name = "BP_PlayerVehicleSpawner"
                rot = unreal.Rotator(0, 0, 0)
            else:
                asset_path = TILE_ASSET_MAP.get(key)
                actor_name = f"Tile_{r}_{c}"
                rot = unreal.Rotator(0, 0, 0)
            if not asset_path:
                continue
            asset = _load_asset(asset_path)
            if asset is None:
                continue
            # global world coordinates: column-major grid, centred sector
            location = unreal.Vector(
                start_location.x + c * TILE_SIZE_UU - (cols * TILE_SIZE_UU) / 2,
                start_location.y + r * TILE_SIZE_UU - (rows * TILE_SIZE_UU) / 2,
                start_location.z,
            )
            actor = actor_subsys.spawn_actor_from_asset(asset, location, rot)
            if actor:
                actor.set_actor_label(actor_name)
                spawned += 1
            else:
                unreal.log_warning(
                    f"[ue5_world_builder] failed to spawn {asset_path} at {location}"
                )

    unreal.log(f"[ue5_world_builder] spawned {spawned} actors into the level")
    return spawned


def build_world(matrix=None):
    """Run the full UE5 automation chain."""
    import_mesh_assets()
    import_audio_assets()
    import_texture_assets()
    override_all_textures()
    return spawn_procedural_map(matrix)


if __name__ == "__main__":
    # Guard: only meaningful inside the editor, but main() can be called
    # from the editor console as well.
    unreal.log("[ue5_world_builder] starting Project Neon-Underground world assembly")
    build_world()
    unreal.log("[ue5_world_builder] world assembly complete")
