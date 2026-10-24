local S = core.get_translator(core.get_current_modname())

core.set_mapgen_setting("mg_name", "v7", true)
core.set_mapgen_setting("water_level", "1", true)
core.set_mapgen_setting("mgv7_spflags", "mountains,ridges,caverns", true)

local function material_node(description, texture, groups, extra)
    local definition = {
        description = S(description),
        tiles = {texture},
        groups = groups,
        is_ground_content = true,
    }
    for key, value in pairs(extra or {}) do
        definition[key] = value
    end
    return definition
end

core.register_node("sw_world:meadow", material_node("Living Meadow", "sw_grass_top.png", {
    crumbly = 2, soil = 1,
}, {
    tiles = {"sw_grass_top.png", "sw_earth.png", "sw_earth.png"},
    drop = "sw_world:earth",
}))

core.register_node("sw_world:earth", material_node("Rich Earth", "sw_earth.png", {
    crumbly = 3, soil = 1,
}))

core.register_node("sw_world:slate", material_node("Weathered Slate", "sw_stone.png", {
    cracky = 3, stone = 1,
}))

core.register_node("sw_world:sand", material_node("River Sand", "sw_sand.png", {
    crumbly = 3, falling_node = 1, sand = 1,
}))

core.register_node("sw_world:timber", material_node("Old Oak Timber", "sw_bark.png", {
    choppy = 2, oddly_breakable_by_hand = 1, flammable = 2, tree = 1,
}, {
    tiles = {"sw_wood_top.png", "sw_wood_top.png", "sw_bark.png"},
    paramtype2 = "facedir",
    on_place = core.rotate_node,
}))

core.register_node("sw_world:canopy", {
    description = S("Oak Canopy"),
    drawtype = "allfaces_optional",
    waving = 1,
    tiles = {"sw_leaves.png"},
    use_texture_alpha = "clip",
    paramtype = "light",
    sunlight_propagates = true,
    groups = {snappy = 3, leafdecay = 3, leaves = 1, flammable = 2},
    drop = {max_items = 1, items = {
        {items = {"sw_world:canopy"}, rarity = 18},
    }},
})

core.register_node("sw_world:water_source", {
    description = S("Clear Water Source"),
    drawtype = "liquid",
    waving = 3,
    tiles = {{name = "sw_water.png", backface_culling = false}},
    special_tiles = {{name = "sw_water.png", backface_culling = false}},
    use_texture_alpha = "blend",
    paramtype = "light",
    walkable = false,
    pointable = false,
    diggable = false,
    buildable_to = true,
    is_ground_content = false,
    drop = "",
    drowning = 1,
    liquidtype = "source",
    liquid_alternative_flowing = "sw_world:water_flowing",
    liquid_alternative_source = "sw_world:water_source",
    liquid_viscosity = 1,
    liquid_range = 8,
    post_effect_color = {a = 88, r = 15, g = 70, b = 96},
    groups = {water = 3, liquid = 3, cools_lava = 1},
})

core.register_node("sw_world:water_flowing", {
    description = S("Flowing Clear Water"),
    drawtype = "flowingliquid",
    waving = 3,
    tiles = {"sw_water.png"},
    special_tiles = {
        {name = "sw_water.png", backface_culling = false},
        {name = "sw_water.png", backface_culling = true},
    },
    use_texture_alpha = "blend",
    paramtype = "light",
    paramtype2 = "flowingliquid",
    walkable = false,
    pointable = false,
    diggable = false,
    buildable_to = true,
    is_ground_content = false,
    drop = "",
    drowning = 1,
    liquidtype = "flowing",
    liquid_alternative_flowing = "sw_world:water_flowing",
    liquid_alternative_source = "sw_world:water_source",
    liquid_viscosity = 1,
    liquid_range = 8,
    post_effect_color = {a = 88, r = 15, g = 70, b = 96},
    groups = {water = 3, liquid = 3, cools_lava = 1, not_in_creative_inventory = 1},
})

core.register_node("sw_world:coal_seam", material_node("Carbon Seam", "sw_coal.png", {
    cracky = 3, stone = 1, ore = 1,
}))

core.register_node("sw_world:iron_seam", material_node("Iron-Bearing Slate", "sw_iron.png", {
    cracky = 2, stone = 1, ore = 1,
}))

core.register_node("sw_world:foundation", material_node("Deep Foundation", "sw_foundation.png", {
    unbreakable = 1,
}, {
    diggable = false,
    drop = "",
}))

core.register_node("sw_world:amber_lantern", {
    description = S("Amber Lantern"),
    tiles = {"sw_sand.png^[colorize:#f3a632:82"},
    paramtype = "light",
    sunlight_propagates = true,
    light_source = 13,
    groups = {cracky = 2, oddly_breakable_by_hand = 2},
})

core.register_alias("mapgen_stone", "sw_world:slate")
core.register_alias("mapgen_water_source", "sw_world:water_source")
core.register_alias("mapgen_river_water_source", "sw_world:water_source")
core.register_alias("mapgen_dirt", "sw_world:earth")
core.register_alias("mapgen_dirt_with_grass", "sw_world:meadow")
core.register_alias("mapgen_sand", "sw_world:sand")
core.register_alias("mapgen_tree", "sw_world:timber")
core.register_alias("mapgen_leaves", "sw_world:canopy")

core.clear_registered_biomes()
core.clear_registered_decorations()
core.clear_registered_ores()

core.register_biome({
    name = "temperate_highlands",
    node_top = "sw_world:meadow",
    depth_top = 1,
    node_filler = "sw_world:earth",
    depth_filler = 4,
    node_stone = "sw_world:slate",
    node_water_top = "sw_world:water_source",
    depth_water_top = 4,
    node_water = "sw_world:water_source",
    node_river_water = "sw_world:water_source",
    y_min = 2,
    y_max = 31000,
    heat_point = 48,
    humidity_point = 72,
})

core.register_biome({
    name = "river_lowlands",
    node_top = "sw_world:meadow",
    depth_top = 1,
    node_filler = "sw_world:earth",
    depth_filler = 5,
    node_stone = "sw_world:slate",
    node_water_top = "sw_world:water_source",
    depth_water_top = 8,
    node_water = "sw_world:water_source",
    node_river_water = "sw_world:water_source",
    y_min = -4,
    y_max = 6,
    heat_point = 55,
    humidity_point = 82,
})

core.register_biome({
    name = "slate_depths",
    node_top = "sw_world:sand",
    depth_top = 2,
    node_filler = "sw_world:slate",
    depth_filler = 2,
    node_stone = "sw_world:slate",
    node_water = "sw_world:water_source",
    y_min = -31000,
    y_max = 1,
    heat_point = 50,
    humidity_point = 50,
})

core.register_ore({
    ore_type = "scatter",
    ore = "sw_world:coal_seam",
    wherein = "sw_world:slate",
    clust_scarcity = 9 * 9 * 9,
    clust_num_ores = 9,
    clust_size = 4,
    y_min = -256,
    y_max = 48,
})

core.register_ore({
    ore_type = "scatter",
    ore = "sw_world:iron_seam",
    wherein = "sw_world:slate",
    clust_scarcity = 12 * 12 * 12,
    clust_num_ores = 6,
    clust_size = 3,
    y_min = -256,
    y_max = 18,
})

local function oak_schematic()
    local size = {x = 5, y = 8, z = 5}
    local data = {}
    for z = 0, size.z - 1 do
        for y = 0, size.y - 1 do
            for x = 0, size.x - 1 do
                local dx, dz = x - 2, z - 2
                local name, prob = "air", 0
                if x == 2 and z == 2 and y <= 5 then
                    name, prob = "sw_world:timber", 255
                elseif y >= 3 and y <= 6 and dx * dx + dz * dz <= (y == 6 and 2 or 5) then
                    name, prob = "sw_world:canopy", (math.abs(dx) + math.abs(dz) >= 3) and 180 or 255
                elseif y == 7 and x == 2 and z == 2 then
                    name, prob = "sw_world:canopy", 255
                end
                data[#data + 1] = {name = name, prob = prob, force_place = false}
            end
        end
    end
    return {size = size, data = data, yslice_prob = {}}
end

core.register_decoration({
    name = "sw_world:old_oak",
    deco_type = "schematic",
    place_on = {"sw_world:meadow"},
    sidelen = 16,
    noise_params = {
        offset = 0.012,
        scale = 0.018,
        spread = {x = 180, y = 180, z = 180},
        seed = 5381,
        octaves = 3,
        persistence = 0.62,
        lacunarity = 2.0,
    },
    biomes = {"temperate_highlands", "river_lowlands"},
    y_min = 2,
    y_max = 180,
    schematic = oak_schematic(),
    flags = "place_center_x,place_center_z",
    rotation = "random",
})

core.register_on_joinplayer(function(player)
    player:set_sky({
        type = "regular",
        clouds = true,
        body_orbit_tilt = -18,
        sky_color = {
            day_sky = "#5e9fc9",
            day_horizon = "#b8d6dc",
            dawn_sky = "#735a74",
            dawn_horizon = "#e29367",
            night_sky = "#071321",
            night_horizon = "#182537",
            indoors = "#252a2c",
            fog_sun_tint = "#ed9b61",
            fog_moon_tint = "#849bb5",
            fog_tint_type = "custom",
        },
        fog = {fog_distance = 110, fog_start = 0.55, fog_color = "#9cb5b7"},
    })
    player:set_sun({visible = true, texture = "", sunrise_visible = true, scale = 1.35})
    player:set_moon({visible = true, texture = "", scale = 0.75})
    player:set_stars({visible = true, count = 1800, star_color = "#e8efffcc", scale = 0.7, star_seed = 5357})
    player:set_clouds({
        density = 0.28,
        color = "#eef2f0db",
        ambient = "#4e5660",
        height = 150,
        thickness = 22,
        speed = {x = -1.2, z = -0.4},
        shadow = "#889196",
    })
    player:set_lighting({
        saturation = 1.08,
        shadows = {intensity = 0.33, tint = {r = 32, g = 39, b = 45}},
        exposure = {
            luminance_min = -4.2,
            luminance_max = -1.4,
            exposure_correction = 0.15,
            speed_dark_bright = 800,
            speed_bright_dark = 500,
            center_weight_power = 1.3,
        },
        bloom = {intensity = 0.08, strength_factor = 1.1, radius = 1.1},
        volumetric_light = {strength = 0.16},
    })
end)
