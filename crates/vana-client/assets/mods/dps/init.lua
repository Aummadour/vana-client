-- /dps prints the damage our party dealt since the first hit after a reset: each member's total, share and damage a
-- second over the fight's span. /dps reset starts again. Every damage number counts (main and proc records), by its
-- actor: us, the party (trusts included) and their pets.

-- The colour of chat mode 0x8b, as the lines always had.
local COLOUR = {colour = 0x8b}

local by, first, last = {}, nil, nil

-- Us, a member of our party, or the pet of either (its master's index).
local function ours(actor)
  local me = game.player()
  if actor == me.id then
    return true
  end
  local members = game.party()
  for _, m in ipairs(members) do
    if m.party == 0 and m.id == actor then
      return true
    end
  end
  local e = game.entity(actor)
  if not e or e.master == 0 then
    return false
  end
  if e.master == me.index then
    return true
  end
  for _, m in ipairs(members) do
    local pe = m.party == 0 and game.entity(m.id)
    if pe and pe.index == e.master then
      return true
    end
  end
  return false
end

local function name(id)
  local e = game.entity(id)
  return e and e.name or ("0x%x"):format(id)
end

events.on("damage", function(actor, _, value)
  if value <= 0 or not ours(actor) then
    return
  end
  local now = clock()
  first = first or now
  last = now
  local e = by[actor] or {dmg = 0, hits = 0}
  e.dmg = e.dmg + value
  e.hits = e.hits + 1
  by[actor] = e
end)

commands.register("dps", function(arg)
  if arg and arg:lower() == "reset" then
    by, first, last = {}, nil, nil
    chat.add("DPS: reset.", COLOUR)
    return
  end
  if not first then
    chat.add("DPS: no damage yet.", COLOUR)
    return
  end
  local secs = math.max(math.floor(last - first), 1)
  local total, rows = 0, {}
  for id, v in pairs(by) do
    total = total + v.dmg
    rows[#rows + 1] = {id = id, dmg = v.dmg, hits = v.hits}
  end
  table.sort(rows, function(a, b) return a.dmg > b.dmg end)
  chat.add(("DPS over %d:%02d: %d damage"):format(secs // 60, secs % 60, total), COLOUR)
  for _, r in ipairs(rows) do
    chat.add(("  %s: %d (%.0f%%), %.1f/s, %d hits"):format(name(r.id), r.dmg, r.dmg * 100 / math.max(total, 1), r.dmg / secs, r.hits), COLOUR)
  end
end)
