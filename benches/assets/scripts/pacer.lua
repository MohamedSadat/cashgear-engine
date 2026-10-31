-- Paces back and forth, reading and writing the transform every step: the cost of a
-- typical script's engine calls.
return {
  on_start = function(self)
    self.t = 0
    self.speed = 20 + (self:id() % 7) * 5
  end,

  on_update = function(self, dt)
    self.t = self.t + dt
    local x, y = self:position()
    local vx, vy = self:velocity()
    if math.floor(self.t) % 2 == 0 then
      self:set_velocity(self.speed, vy)
    else
      self:set_velocity(-self.speed, vy)
    end
    self:set_position(x, y)
  end,
}
