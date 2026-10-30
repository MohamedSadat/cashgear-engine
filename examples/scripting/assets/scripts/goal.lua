-- The goal flag: reaching it wins. The game shows the win screen when its state changes
-- (`state.set`), and every script hears "won" (the demo's bot stops there).
local goal = {}

function goal.on_collision(self, other)
  if self.reached or other:name() ~= "player" then
    return
  end
  self.reached = true
  audio.play("sounds/goal.ogg", { volume = 0.7 })
  self:tween("scale", 1, 0.4, { from = 1.5, ease = "BackOut" })
  events.broadcast("won")
  state.set("Won")
  log.info(("reached the goal after %.1f s"):format(time.now))
end

return goal
