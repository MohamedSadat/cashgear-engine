-- A red block that patrols the ground, turning at walls and ledges. Bumping into the
-- player does no harm; it just flashes.
local patrol = {}

function patrol.on_start(self)
  self.dir = -1
  self.speed = self:props().speed or 30
end

-- Whether there is ground a little ahead of the feet.
local function ground_ahead(self)
  local x, y = self:world_position()
  local ahead = { exclude = self, sensors = false }
  return world.raycast(x + self.dir * 9, y + 2, 0, -1, 10, ahead) ~= nil
end

function patrol.on_update(self, dt)
  local _, y = self:world_position()
  if y < -100 then
    self:despawn()
    return
  end
  local c = self:contacts()
  local blocked = (self.dir < 0 and c.wall_left) or (self.dir > 0 and c.wall_right)
  if blocked or (c.ground and not ground_ahead(self)) then
    self.dir = -self.dir
  end
  self:set_velocity(self.dir * self.speed, nil)
end

function patrol.on_collision(self, other)
  if other:name() == "player" then
    self:tween("flash", "#ffffff", 0.1, { yoyo = true, repeats = 1 })
  end
end

return patrol
