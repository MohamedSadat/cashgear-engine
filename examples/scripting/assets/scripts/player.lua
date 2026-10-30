-- The player, all in Lua: running with acceleration, a jump that is higher while the
-- button is held, squash and stretch, spikes that knock it back, and the animation
-- state machine's parameters. In the demo (`--demo`) a bot below plays instead of the
-- controls.
--
-- Save this file while the example runs to change how it plays: the speeds below, say.
local player = {}

local RUN_SPEED = 120
local RUN_ACCEL = 900
local JUMP_SPEED = 330
-- Rising speed is multiplied by this in each step the jump button is up.
local JUMP_CUT = 0.5
-- Spikes throw the alien back and up at this speed.
local KNOCK_X, KNOCK_Y = 150, 240
-- Seconds without control after a hit, and without further hits.
local STUNNED, INVINCIBLE = 0.35, 1.5

function player.on_start(self)
  self.picture = self:child("picture")
  self.facing = 1
  self.stunned = 0
  self.invincible = 0
  self.fall_speed = 0
  self.start_x, self.start_y = self:position()
  -- Tell the game to point the camera at us.
  events.emit("follow")
end

local function move_towards(from, to, step)
  if math.abs(to - from) <= step then
    return to
  end
  return from + (to > from and step or -step)
end

-- What the player wants this step: a run direction, a jump, and whether jump is held.
local function controls()
  return input.axis("left", "right"), input.just_pressed("jump"), input.pressed("jump")
end

-- The demo's player: runs right, jumps at walls it finds with a ray, and follows the
-- "jump" hints placed in the map.
local function bot(self, contacts)
  if self.hint and time.step > self.hint_at + 30 then
    self.hint = nil -- Hints left unused for half a second are forgotten.
  end
  if self.done then
    return 0, false, false
  end
  local held = (self.hold or 0) > 0
  self.hold = math.max((self.hold or 0) - 1, 0)
  local jump = false
  if contacts.ground then
    if self.hint == "jump" then
      jump = true
    else
      -- A wall right ahead: a hit facing back at us. Slopes face up, so they do not count.
      local x, y = self:world_position()
      local hit = world.raycast(x + 5, y + 4, 1, 0, 6, { exclude = self, sensors = false })
      jump = hit ~= nil and hit.nx < -0.9
    end
  end
  if jump then
    self.hint = nil
    held = true
    self.hold = 20
  end
  return 1, jump, held
end

function player.on_collision(self, other)
  local hint = other:has("TiledObject") and other:props().action
  if hint then
    self.hint, self.hint_at = hint, time.step
  end
end

function player.on_event(self, name)
  if name == "won" then
    self.done = true
  end
end

local function hurt(self)
  self.stunned, self.invincible = STUNNED, INVINCIBLE
  self:set_velocity(-self.facing * KNOCK_X, KNOCK_Y)
  self.picture:trigger("hurt")
  self.picture:tween("flash", "#ffffffd0", 0.12, { yoyo = true, repeats = 1 })
  audio.play("sounds/hit.ogg", { volume = 0.6 })
end

function player.on_update(self, dt)
  self.stunned = math.max(self.stunned - dt, 0)
  self.invincible = math.max(self.invincible - dt, 0)
  local contacts = self:contacts()
  local vx, vy = self:velocity()

  if contacts.landed then
    -- Squash in proportion to the fall.
    local s = math.min(math.max(self.fall_speed / 480, 0.3), 1)
    self.picture:tween("scale", 1, 0.3, { from = { 1 + 0.35 * s, 1 - 0.3 * s }, ease = "BackOut" })
    if self.fall_speed > 200 then
      audio.play("sounds/land.ogg", { volume = 0.4 })
    end
  end
  self.fall_speed = contacts.ground and 0 or math.max(self.fall_speed, -vy)

  local _, y = self:position()
  if y < -60 then
    -- Fell out of the level: back to the start.
    self:set_position(self.start_x, self.start_y)
    self:set_velocity(0, 0)
    return
  end
  if contacts.hazard and self.invincible <= 0 then
    hurt(self)
  end

  if self.stunned <= 0 then
    local run, jump, held
    if game.demo() then
      run, jump, held = bot(self, contacts)
    else
      run, jump, held = controls()
    end
    local accel = contacts.ground and RUN_ACCEL or RUN_ACCEL * 0.6
    vx = move_towards(vx, run * RUN_SPEED, accel * dt)
    if run ~= 0 and (run > 0 and 1 or -1) ~= self.facing then
      self.facing = run > 0 and 1 or -1
      self.picture:set_flip_x(self.facing < 0)
    end
    if contacts.ground and jump then
      vy = JUMP_SPEED
      self.picture:tween("scale", 1, 0.25, { from = { 0.8, 1.25 }, ease = "QuadOut" })
      audio.play("sounds/jump.ogg", { volume = 0.5, pitch_jitter = 0.5 })
    end
    if vy > 0 and not held then
      vy = vy * JUMP_CUT
    end
    self:set_velocity(vx, vy)
  end

  -- The animation state machine (anims/alien.anim.ron) picks the clip from these.
  self.picture:set_param("speed", math.abs(vx))
  self.picture:set_param("vy", vy)
  self.picture:set_param("grounded", contacts.ground)
end

return player
