-- A checkpoint flag: grey until the player reaches it, then it lights up with a pop, a
-- burst of sparks and a chime, and a fall brings the player back here.
--
-- `prefabs/checkpoint.prefab.ron` gives the flag its sensor and this script; the map's
-- `Checkpoint` objects get the prefab by their class. The Rust version is the checkpoint
-- branch of `things::touch_things`, which the web build uses.
local checkpoint = {}

-- A flag not reached yet.
local GREY = 0x8b9bb4

function checkpoint.on_start(self)
  self.active = false
  self:set_color(GREY)
end

function checkpoint.on_collision(self, other)
  if self.active or not other:has("Player") or not game.player_active() then
    return
  end
  self.active = true
  -- The flag is one tile above the ground; come back standing under it.
  local x, y = self:world_position()
  y = y - 18
  game.set_checkpoint(x, y)
  self:tween("color", 0xffffff, 0.3)
  self:tween("scale", 1, 0.4, { from = 1.4, ease = "BackOut" })
  self:insert_prefab("CheckpointSparks")
  audio.play("sounds/checkpoint.ogg", { volume = 0.7 })
  log.info(("checkpoint at [%g, %g]"):format(x, y))
end

return checkpoint
