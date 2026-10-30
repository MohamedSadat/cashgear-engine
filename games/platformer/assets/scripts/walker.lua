-- A walker (the blue enemy): it patrols, turning at walls, at ledges, at spikes and at the
-- end of its beat. Stomping it is the game's business (in Rust); it tells this script with
-- a "stomped" event.
--
-- The Rust version is the walker branch of `enemies::think`, which the web build uses
-- (Lua does not run in browsers). A test plays the whole game with each and checks that
-- every walker is in the same place at every step. Save this file while the game runs to
-- reload it: the walkers keep where they are and which way they face.
local walker = {}

-- The tiles' and static colliders' collision layer.
local WORLD = 1

function walker.on_start(self)
  local props = self:props()
  -- -1 left, 1 right.
  self.dir = (props.dir or -1) < 0 and -1 or 1
  self.speed = props.speed or 28
  -- How far it may go from home either way; walls, ledges and spikes turn it anyway.
  self.range = math.max(props.range or math.huge, 1)
  self.home = self:position()
end

-- Whether there is floor, and no spikes, a little ahead of the feet.
local function floor_ahead(self, x, y)
  local solid = { mask = WORLD, exclude = self, sensors = false }
  local foot_x, foot_y = x + self.dir * 9, y - 12
  local floor = world.raycast(foot_x, foot_y + 2, 0, -1, 10, solid) ~= nil
  local tile = world.tile_at(foot_x, foot_y + 6, solid)
  return floor and not (tile and tile.hazard)
end

function walker.on_update(self, dt)
  if self.dead then
    return
  end
  local x, y = self:world_position()
  if game.fell(y) then
    self:despawn()
    return
  end
  local contacts = self:contacts()
  local blocked = (self.dir < 0 and contacts.wall_left) or (self.dir > 0 and contacts.wall_right)
  local far = (x - self.home) * self.dir > self.range
  if blocked or far or (contacts.ground and not floor_ahead(self, x, y)) then
    self.dir = -self.dir
  end
  self:set_velocity(self.dir * self.speed, nil)
  -- Kenney's characters face left.
  self:set_flip_x(self.dir > 0)
end

function walker.on_event(self, name)
  if name == "stomped" then
    self.dead = true
    self:play("walker_dead")
  end
end

return walker
