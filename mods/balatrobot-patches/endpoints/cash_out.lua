-- src/lua/endpoints/cash_out.lua

-- ==========================================================================
-- CashOut Endpoint Params
-- ==========================================================================

---@class Request.Endpoint.CashOut.Params

-- ==========================================================================
-- CashOut Endpoint
-- ==========================================================================

-- Local patch (JEV): wait for the game's queued (blocking) events to finish, both before cashing out
-- and before replying. Otherwise a fast client can
--   1. cash out before the round eval's last row sets G.GAME.current_round.dollars (common_events.lua
--      add_round_eval_row 'bottom'), so the previous round's total is paid;
--   2. read the shop before tag effects land (tag.lua Tag:yep: edition/Rare/Uncommon tags set the
--      edition, visibility and $0 cost in queued events).
-- A fallback after IDLE_TIMEOUT seconds keeps the endpoint from ever hanging.
-- The unpatched file is cash_out.lua.orig.

local IDLE_TIMEOUT = 3

local function base_queue_idle()
  local q = G.E_MANAGER and G.E_MANAGER.queues and G.E_MANAGER.queues.base
  if not q then
    return true
  end
  for _, ev in ipairs(q) do
    if ev.blocking then
      return false
    end
  end
  return true
end

local function has_cash_out_button()
  for _, b in ipairs(G.I.UIBOX) do
    if b:get_UIE_by_ID("cash_out_button") then
      return true
    end
  end
  return false
end

---@type Endpoint
return {

  name = "cash_out",

  description = "Cash out and collect round rewards",

  schema = {},

  requires_state = { G.STATES.ROUND_EVAL },

  ---@param _ Request.Endpoint.CashOut.Params
  ---@param send_response fun(response: Response.Endpoint)
  execute = function(_, send_response)
    sendDebugMessage("Init cash_out()", "BB.ENDPOINTS")

    local num_items = function(area)
      local count = 0
      if area and area.cards then
        for _, v in ipairs(area.cards) do
          if v.children.buy_button and v.children.buy_button.definition then
            count = count + 1
          end
        end
      end
      return count
    end

    -- Wait for SHOP state after state transition completes, and for tag effects to land
    local function wait_for_shop()
      local since = nil
      G.E_MANAGER:add_event(Event({
        trigger = "condition",
        blocking = false,
        func = function()
          if G.STATE == G.STATES.SHOP and G.STATE_COMPLETE then
            local ready = num_items(G.shop_booster) > 0 or num_items(G.shop_jokers) > 0 or num_items(G.shop_vouchers) > 0
            if ready then
              since = since or love.timer.getTime()
              local timed_out = love.timer.getTime() - since > IDLE_TIMEOUT
              if base_queue_idle() or timed_out then
                if timed_out then
                  sendWarnMessage("cash_out(): shop events still pending after timeout", "BB.ENDPOINTS")
                end
                sendDebugMessage("Return cash_out() - reached SHOP state", "BB.ENDPOINTS")
                send_response(BB_GAMESTATE.get_gamestate())
                return true
              end
            end
          end
          return false
        end,
      }))
    end

    -- Cash out only once the round eval has finished adding its rows (the last one sets the payout)
    local start = love.timer.getTime()
    G.E_MANAGER:add_event(Event({
      trigger = "condition",
      blocking = false,
      func = function()
        local timed_out = love.timer.getTime() - start > IDLE_TIMEOUT
        if (has_cash_out_button() and base_queue_idle()) or timed_out then
          if timed_out then
            sendWarnMessage("cash_out(): round eval events still pending after timeout", "BB.ENDPOINTS")
          end
          G.FUNCS.cash_out({ config = {} })
          wait_for_shop()
          return true
        end
        return false
      end,
    }))
  end,
}
