-- A coin: it bobs, and when the player touches it, it chimes, pops and tells the game.
local coin = {}

function coin.on_start(self)
  local x, y = self:position()
  -- Coins along a row bob one after another.
  self:tween("position", { x, y + 2 }, 0.7, {
    from = { x, y },
    ease = "SineInOut",
    yoyo = true,
    forever = true,
    delay = (x / 18) % 4 * 0.15,
  })
end

function coin.on_collision(self, other)
  if self.taken or other:name() ~= "player" then
    return
  end
  self.taken = true
  events.emit("coin")
  audio.play("sounds/coin.ogg", { volume = 0.5, pitch_jitter = 0.3 })
  self:tween("scale", 1.8, 0.25, { ease = "QuadOut" })
  self:tween("color", "#ffffff00", 0.25, { ease = "QuadIn" })
  self:after(0.25, function(me)
    me:despawn()
  end)
end

return coin
