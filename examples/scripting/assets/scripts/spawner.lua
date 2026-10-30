-- Drops its `drops` prefab every `every` seconds, a little to the left or right, and
-- keeps at most five of them: the oldest goes when a sixth falls.
local spawner = {}

function spawner.on_start(self)
  local props = self:props()
  self.dropped = {}
  self:every(props.every or 3, function(me)
    local x, y = me:world_position()
    local crate = world.spawn(props.drops or "Crate", x + math.random(-40, 40), y)
    table.insert(me.dropped, crate)
    if #me.dropped > 5 then
      table.remove(me.dropped, 1):send("go")
    end
  end)
end

return spawner
