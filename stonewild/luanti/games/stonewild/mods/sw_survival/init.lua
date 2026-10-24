local S = core.get_translator(core.get_current_modname())
local nourishment_hud = {}
local vitality_hud = {}

local function formspec()
    return table.concat({
        "formspec_version[6]",
        "size[9.1,8.8]",
        "no_prepend[]",
        "bgcolor[#10191ddd;true]",
        "background9[0,0;9.1,8.8;sw_earth.png;true;12]",
        "label[0.55,0.35;FIELD CRAFTING]",
        "list[current_player;craft;1.0,0.75;3,3;0]",
        "image[4.35,1.55;0.7,0.7;sw_stone.png^[resize:32x32]",
        "label[4.44,2.33;→]",
        "list[current_player;craftpreview;5.45,1.5;1,1;0]",
        "label[0.55,3.85;PACK]",
        "list[current_player;main;0.55,4.25;8,4;0]",
        "listring[current_player;main]",
        "listring[current_player;craft]",
    })
end

local function nourishment(player)
    local value = player:get_meta():get_int("sw_nourishment")
    if value <= 0 and player:get_meta():get_string("sw_nourishment_ready") ~= "yes" then
        value = 20
        player:get_meta():set_int("sw_nourishment", value)
        player:get_meta():set_string("sw_nourishment_ready", "yes")
    end
    return value
end

local function set_nourishment(player, amount)
    amount = math.max(0, math.min(20, math.floor(amount)))
    player:get_meta():set_int("sw_nourishment", amount)
    local id = nourishment_hud[player:get_player_name()]
    if id then
        player:hud_change(id, "number", amount)
    end
end

local function eat_berries(itemstack, user)
    if not user or not user:is_player() then
        return itemstack
    end
    set_nourishment(user, nourishment(user) + 4)
    user:set_hp(math.min(user:get_properties().hp_max or 20, user:get_hp() + 1), {
        type = "set_hp", from = "mod", _stonewild = true,
    })
    if not core.is_creative_enabled(user:get_player_name()) then
        itemstack:take_item()
    end
    return itemstack
end

core.register_node("sw_survival:oak_planks", {
    description = S("Oak Planks"),
    tiles = {"sw_bark.png^[colorize:#b98655:75"},
    paramtype2 = "facedir",
    groups = {choppy = 2, oddly_breakable_by_hand = 2, flammable = 3, wood = 1},
    on_place = core.rotate_node,
})

core.register_craftitem("sw_survival:branch", {
    description = S("Oak Branch"),
    inventory_image = "sw_bark.png^[resize:16x16",
    groups = {stick = 1, flammable = 1},
})

core.register_craftitem("sw_survival:wild_berries", {
    description = S("Wild Berries"),
    inventory_image = "sw_berry_bush.png",
    on_use = eat_berries,
    groups = {food = 1},
})

core.register_craftitem("sw_survival:carbon", {
    description = S("Carbon Chunk"),
    inventory_image = "sw_coal.png^[resize:48x48",
})

core.register_craftitem("sw_survival:raw_iron", {
    description = S("Raw Iron"),
    inventory_image = "sw_iron.png^[resize:48x48",
})

core.register_node("sw_survival:berry_bush", {
    description = S("Wild Berry Bush"),
    drawtype = "plantlike",
    waving = 1,
    tiles = {"sw_berry_bush.png"},
    inventory_image = "sw_berry_bush.png",
    wield_image = "sw_berry_bush.png",
    paramtype = "light",
    sunlight_propagates = true,
    walkable = false,
    buildable_to = true,
    selection_box = {type = "fixed", fixed = {-0.4, -0.5, -0.4, 0.4, 0.38, 0.4}},
    groups = {snappy = 3, flora = 1, attached_node = 1, flammable = 2},
    drop = {
        max_items = 2,
        items = {
            {items = {"sw_survival:wild_berries 2"}},
            {items = {"sw_survival:wild_berries"}, rarity = 2},
        },
    },
})

core.register_tool("sw_survival:stone_pick", {
    description = S("Slate Pick"),
    inventory_image = "sw_stone_pick.png",
    range = 4.5,
    tool_capabilities = {
        full_punch_interval = 1.05,
        max_drop_level = 1,
        groupcaps = {cracky = {times = {[1] = 3.8, [2] = 1.6, [3] = 0.75}, uses = 34, maxlevel = 1}},
        damage_groups = {fleshy = 3},
    },
})

core.register_tool("sw_survival:stone_axe", {
    description = S("Slate Hatchet"),
    inventory_image = "sw_stone_axe.png",
    range = 4.5,
    tool_capabilities = {
        full_punch_interval = 0.95,
        max_drop_level = 1,
        groupcaps = {choppy = {times = {[1] = 3.1, [2] = 1.25, [3] = 0.65}, uses = 34, maxlevel = 1}},
        damage_groups = {fleshy = 4},
    },
})

core.register_tool("sw_survival:stone_shovel", {
    description = S("Slate Spade"),
    inventory_image = "sw_stone_shovel.png",
    range = 4.5,
    tool_capabilities = {
        full_punch_interval = 1.0,
        max_drop_level = 1,
        groupcaps = {crumbly = {times = {[1] = 2.0, [2] = 0.85, [3] = 0.35}, uses = 38, maxlevel = 1}},
        damage_groups = {fleshy = 2},
    },
})

core.register_tool("sw_survival:stone_blade", {
    description = S("Slate Blade"),
    inventory_image = "sw_stone_blade.png",
    range = 4.2,
    tool_capabilities = {
        full_punch_interval = 0.72,
        max_drop_level = 1,
        groupcaps = {snappy = {times = {[1] = 2.2, [2] = 0.9, [3] = 0.28}, uses = 38, maxlevel = 1}},
        damage_groups = {fleshy = 6},
    },
})

core.override_item("sw_world:coal_seam", {drop = "sw_survival:carbon"})
core.override_item("sw_world:iron_seam", {drop = "sw_survival:raw_iron"})
core.override_item("sw_world:canopy", {
    drop = {max_items = 1, items = {
        {items = {"sw_survival:wild_berries"}, rarity = 22},
        {items = {"sw_world:canopy"}, rarity = 12},
    }},
})

core.register_item(":", {
    type = "none",
    wield_image = "sw_earth.png^[resize:48x48^[opacity:80",
    wield_scale = {x = 1, y = 1, z = 2.5},
    tool_capabilities = {
        full_punch_interval = 0.9,
        max_drop_level = 0,
        groupcaps = {
            crumbly = {times = {[2] = 3.0, [3] = 0.75}, uses = 0, maxlevel = 1},
            oddly_breakable_by_hand = {times = {[1] = 4.0, [2] = 2.0, [3] = 0.7}, uses = 0},
            snappy = {times = {[3] = 0.8}, uses = 0, maxlevel = 1},
        },
        damage_groups = {fleshy = 1},
    },
})

core.register_craft({output = "sw_survival:oak_planks 4", recipe = {{"sw_world:timber"}}})
core.register_craft({output = "sw_survival:branch 4", recipe = {{"sw_survival:oak_planks"}, {"sw_survival:oak_planks"}}})
core.register_craft({output = "sw_survival:stone_pick", recipe = {
    {"sw_world:slate", "sw_world:slate", "sw_world:slate"},
    {"", "sw_survival:branch", ""},
    {"", "sw_survival:branch", ""},
}})
core.register_craft({output = "sw_survival:stone_axe", recipe = {
    {"sw_world:slate", "sw_world:slate", ""},
    {"sw_world:slate", "sw_survival:branch", ""},
    {"", "sw_survival:branch", ""},
}})
core.register_craft({output = "sw_survival:stone_shovel", recipe = {
    {"", "sw_world:slate", ""},
    {"", "sw_survival:branch", ""},
    {"", "sw_survival:branch", ""},
}})
core.register_craft({output = "sw_survival:stone_blade", recipe = {
    {"", "sw_world:slate", ""},
    {"", "sw_world:slate", ""},
    {"", "sw_survival:branch", ""},
}})
core.register_craft({output = "sw_world:amber_lantern 4", recipe = {
    {"", "sw_survival:carbon", ""},
    {"", "sw_survival:branch", ""},
}})

core.register_decoration({
    name = "sw_survival:berry_patches",
    deco_type = "simple",
    place_on = {"sw_world:meadow"},
    sidelen = 16,
    fill_ratio = 0.0018,
    biomes = {"temperate_highlands", "river_lowlands"},
    y_min = 2,
    y_max = 160,
    decoration = "sw_survival:berry_bush",
})

core.register_on_joinplayer(function(player)
    local name = player:get_player_name()
    local inv = player:get_inventory()
    inv:set_size("main", 32)
    inv:set_size("craft", 9)
    inv:set_size("craftpreview", 1)
    player:set_inventory_formspec(formspec())
    player:hud_set_hotbar_itemcount(8)
    player:hud_set_flags({healthbar = false, breathbar = true})

    vitality_hud[name] = player:hud_add({
        type = "statbar",
        position = {x = 0.02, y = 0.94},
        text = "sw_vitality.png",
        number = player:get_hp(),
        item = 20,
        direction = 0,
        size = {x = 24, y = 24},
        offset = {x = 0, y = -54},
        z_index = 100,
    })
    nourishment_hud[name] = player:hud_add({
        type = "statbar",
        position = {x = 0.02, y = 0.94},
        text = "sw_nourishment.png",
        number = nourishment(player),
        item = 20,
        direction = 0,
        size = {x = 24, y = 24},
        offset = {x = 0, y = -27},
        z_index = 100,
    })

    local meta = player:get_meta()
    if meta:get_int("sw_started") == 0 then
        meta:set_int("sw_started", 1)
        inv:add_item("main", "sw_survival:stone_pick")
        inv:add_item("main", "sw_survival:stone_axe")
        inv:add_item("main", "sw_survival:wild_berries 6")
        inv:add_item("main", "sw_world:amber_lantern 8")
        core.chat_send_player(name, S("Welcome to Stonewild Realms. Gather timber and slate before nightfall."))
    end
end)

core.register_on_leaveplayer(function(player)
    local name = player:get_player_name()
    nourishment_hud[name] = nil
    vitality_hud[name] = nil
end)

core.register_on_player_hpchange(function(player, hp_change)
    core.after(0, function()
        if not player or not player:is_player() then return end
        local id = vitality_hud[player:get_player_name()]
        if id then
            player:hud_change(id, "number", player:get_hp())
        end
    end)
    return hp_change
end, true)

core.register_on_dieplayer(function(player)
    set_nourishment(player, 14)
end)

local survival_clock = 0
core.register_globalstep(function(dtime)
    survival_clock = survival_clock + dtime
    if survival_clock < 45 then return end
    survival_clock = survival_clock - 45
    for _, player in ipairs(core.get_connected_players()) do
        local food = nourishment(player)
        local control = player:get_player_control()
        local moving = control.up or control.down or control.left or control.right or control.jump
        if moving or math.random(1, 3) == 1 then
            food = food - 1
            set_nourishment(player, food)
        end
        if food <= 0 then
            player:set_hp(player:get_hp() - 1, {type = "set_hp", from = "mod", _stonewild = true})
        elseif food >= 14 and player:get_hp() < (player:get_properties().hp_max or 20) then
            player:set_hp(player:get_hp() + 1, {type = "set_hp", from = "mod", _stonewild = true})
            if math.random(1, 2) == 1 then
                set_nourishment(player, food - 1)
            end
        end
    end
end)
