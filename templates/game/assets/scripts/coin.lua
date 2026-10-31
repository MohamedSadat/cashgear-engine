-- Coins bob up and down. Lua scripts run on desktop and Android; on the web there is no
-- Lua, and the coins stay still (a game that needs a scripted behaviour on the web keeps a
-- Rust version too). Save this file while the game runs and the coins change at once.
return {
  on_start = function(self)
    local x, y = self:position()
    self.y = y
    -- Neighbouring coins start a little apart, so they ripple.
    self.t = x / 18 * 0.6
  end,

  on_update = function(self, dt)
    self.t = self.t + dt * 4
    local x = self:position()
    self:set_position(x, self.y + math.sin(self.t) * 1.5)
  end,
}
