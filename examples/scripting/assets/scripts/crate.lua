-- A dropped crate: it lands with a bump and fades away when its spawner says so, or after
-- twenty seconds.
local crate = {}

local function fade(self)
  if self.fading then
    return
  end
  self.fading = true
  self:tween("color", "#b86f5000", 0.5)
  self:after(0.5, function(me)
    me:despawn()
  end)
end

function crate.on_start(self)
  self:after(20, fade)
end

function crate.on_update(self)
  if self:contacts().landed then
    self:tween("scale", 1, 0.2, { from = { 1.3, 0.75 }, ease = "BackOut" })
  end
end

function crate.on_event(self, name)
  if name == "go" then
    fade(self)
  end
end

return crate
