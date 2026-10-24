-- Lightweight load-time contract test for the original Stonewild Lua game.
-- Run with Lua 5.1: lua tests/load_game.lua games/stonewild
local game = assert(arg[1], "game path required")
local active_mod = ""
local callbacks = {}

core = {
    registered_nodes = {},
    registered_items = {},
    registered_tools = {},
    registered_crafts = {},
    registered_biomes = {},
    registered_decorations = {},
    registered_ores = {},
}
minetest = core

function core.get_current_modname() return active_mod end
function core.get_translator() return function(text) return text end end
function core.set_mapgen_setting() return true end
function core.register_node(name, def) core.registered_nodes[name] = def; core.registered_items[name] = def end
function core.register_craftitem(name, def) core.registered_items[name] = def end
function core.register_tool(name, def) core.registered_tools[name] = def; core.registered_items[name] = def end
function core.register_item(name, def) core.registered_items[name] = def end
function core.override_item(name, patch)
    local def = assert(core.registered_items[name], "cannot override missing item " .. name)
    for key, value in pairs(patch) do def[key] = value end
end
function core.register_alias() end
function core.clear_registered_biomes() core.registered_biomes = {} end
function core.clear_registered_decorations() core.registered_decorations = {} end
function core.clear_registered_ores() core.registered_ores = {} end
function core.register_biome(def) table.insert(core.registered_biomes, def) end
function core.register_decoration(def) table.insert(core.registered_decorations, def) end
function core.register_ore(def) table.insert(core.registered_ores, def) end
function core.register_craft(def) table.insert(core.registered_crafts, def) end
function core.register_on_joinplayer(fn) table.insert(callbacks, {"join", fn}) end
function core.register_on_leaveplayer(fn) table.insert(callbacks, {"leave", fn}) end
function core.register_on_player_hpchange(fn) table.insert(callbacks, {"hp", fn}) end
function core.register_on_dieplayer(fn) table.insert(callbacks, {"die", fn}) end
function core.register_globalstep(fn) table.insert(callbacks, {"step", fn}) end
function core.rotate_node() end
function core.after() end
function core.chat_send_player() end
function core.is_creative_enabled() return false end
function core.get_connected_players() return {} end

active_mod = "sw_world"
dofile(game .. "/mods/sw_world/init.lua")
active_mod = "sw_survival"
dofile(game .. "/mods/sw_survival/init.lua")

local function present(table_, key)
    assert(table_[key], "missing registration: " .. key)
end

present(core.registered_nodes, "sw_world:meadow")
present(core.registered_nodes, "sw_world:water_source")
present(core.registered_nodes, "sw_world:timber")
present(core.registered_nodes, "sw_survival:berry_bush")
present(core.registered_tools, "sw_survival:stone_pick")
present(core.registered_tools, "sw_survival:stone_axe")
present(core.registered_tools, "sw_survival:stone_shovel")
present(core.registered_tools, "sw_survival:stone_blade")
assert(#core.registered_biomes == 3, "expected three biomes")
assert(#core.registered_ores == 2, "expected two ore registrations")
assert(#core.registered_decorations == 2, "expected oak and berry decorations")
assert(#core.registered_crafts >= 6, "expected survival crafting recipes")
assert(#callbacks >= 6, "expected survival and presentation callbacks")
assert(core.registered_nodes["sw_world:coal_seam"].drop == "sw_survival:carbon")
assert(core.registered_nodes["sw_world:iron_seam"].drop == "sw_survival:raw_iron")

local texture_dir = game .. "/mods/sw_world/textures/"
local survival_texture_dir = game .. "/mods/sw_survival/textures/"
local checked = {}
local function check_texture(value)
    if type(value) == "table" then value = value.name end
    if type(value) ~= "string" or value == "" then return end
    local filename = value:match("([%w_%-]+%.png)")
    if not filename or checked[filename] then return end
    checked[filename] = true
    local file = io.open(texture_dir .. filename, "rb") or io.open(survival_texture_dir .. filename, "rb")
    assert(file, "missing texture: " .. filename)
    file:close()
end
for _, def in pairs(core.registered_items) do
    check_texture(def.inventory_image)
    check_texture(def.wield_image)
    for _, tile in ipairs(def.tiles or {}) do check_texture(tile) end
    for _, tile in ipairs(def.special_tiles or {}) do check_texture(tile) end
end

local texture_count = 0
for _ in pairs(checked) do texture_count = texture_count + 1 end
assert(texture_count >= 15, "too few referenced textures were validated")
print(string.format(
    "Stonewild load contract passed: %d items, %d biomes, %d decorations, %d textures",
    (function() local n=0 for _ in pairs(core.registered_items) do n=n+1 end return n end)(),
    #core.registered_biomes, #core.registered_decorations, texture_count
))
