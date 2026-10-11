-- JEV Tweaks: keep Balatro from blocking an unattended bot.
--
-- When the game returns to the main menu, unlock_notify() queues create_unlock_overlay() for every
-- newly unlocked card. Each one opens an overlay and sets G.SETTINGS.paused = true, so BalatroBot's
-- `start` never reaches blind select and the API hangs. Unlocks are still granted and saved (see the
-- Collection); we only skip the popup.
function create_unlock_overlay(key)
  sendInfoMessage("Skipped unlock popup for " .. tostring(key), "JEV.TWEAKS")
end

sendInfoMessage("JEV Tweaks loaded: unlock popups will be skipped", "JEV.TWEAKS")
