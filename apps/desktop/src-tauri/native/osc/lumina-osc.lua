-- lumina-osc.lua — Lumina 极简自写 OSC（替代 stock OSC）
--
-- mpv v0.41，自带 Lua 可直接用。只依赖 `mp.assdraw` + `mp` 标准 API，
-- 无任何外部依赖（不读 mp.utils / mp.options / 第三方脚本）。
-- 加载方式：libmpv 以 `osc=no` 启动并用 `script=<本文件绝对路径>` 加载；
-- 右键 SUB/AUD 只走自家 `toggle_menu`，永不调原生 select.lua。
--
-- 元素（只要这些，不要上下集/上下章/全屏）：
--   顶部：字幕/音轨下拉选择（含关闭选项）
--   底部：播放/暂停、进度条（点击+拖拽）、时间、音量竖向滑杆（含静音）
-- 输入由宿主 WndProc 通过唯一的 `script-message lumina-surface-mouse ...`
-- 转发。本脚本只消费明确的 move/down/up/click/double/leave/cancel 等事件，
-- 不依赖 mpv keymap。

-- ============================================================================
-- 样式集中调节区（只改这里，不用碰下面的绘制逻辑）
-- 行号说明：本节约 20-70 行，所有外观 knob 都在这里。
-- ============================================================================
local STYLE = {
    -- 字体：改名字即换字体，改数字即调字号（单位 px）
    font = "sans-serif", -- OSD 字体名（如 "Arial" / "Microsoft YaHei"）
    font_size = 22, -- 底部按钮与时间的字号，调大调小改这里
    top_font_size = 18, -- 顶部字幕/音轨按钮的字号

    -- 颜色：ASS 用的是 BBGGRR 顺序（和日常 RGB 相反）！
    -- 白 FFFFFF / 黑 000000 / 红 0000FF / 绿 00FF00 / 蓝 FF0000 / 橙 00A5FF
    col_text = "FFFFFF", -- 按钮文字与时间颜色
    col_bg = "000000", -- 顶部/底部控制条背景色
    col_bar_bg = "555555", -- 进度条底色（未播放部分）
    col_bar_fill = "00A5FF", -- 进度条填充色（已播放部分，默认橙色）
    col_hover = "FFCC66", -- 鼠标悬停时按钮描边高亮色（浅橙）
    col_panel = "171A20", -- 下拉面板背景
    col_panel_border = "3A414D", -- 下拉面板边框
    col_selected = "00A5FF", -- 选中项强调色

    -- 透明度：ASS alpha，0=不透明，255=全透明（十六进制不用管，填十进制）
    alpha_bg = 160, -- 顶部控制条背景透明度，越大越透；想要实底填 0
    alpha_bottom_bg = 255, -- 底部不铺黑色蒙层；255=完全透明
    alpha_panel = 24, -- 浮层背景透明度
    alpha_bar = 40, -- 进度条底色的额外透明度；想要实色填 0

    -- 尺寸与边距（单位 px，相对 OSD 分辨率）
    margin = 16, -- 控制条离窗口四边的距离
    bottom_h = 68, -- 底部控制条高度（含进度条+按钮一整条）
    top_btn_w = 110, -- 顶部字幕/音轨按钮宽度
    top_btn_h = 34, -- 顶部字幕/音轨按钮高度
    play_btn_w = 88, -- 底部播放/暂停按钮宽度
    vol_btn_w = 110, -- 底部音量按钮宽度
    time_label_w = 190, -- 底部时间标签宽度（"88:88 / 88:88" 太长会被裁，改大这里）
    seek_h = 10, -- 进度条粗细（高度）
    radius = 8, -- 圆角半径，0=直角；圆角只影响背景与按钮外框
    gap = 12, -- 底部条内各元素之间的水平间距
    menu_w = 286, -- 字幕/音轨下拉面板宽度
    menu_row_h = 32, -- 下拉选项行高
    volume_popup_w = 58, -- 音量竖条浮层宽度
    volume_popup_h = 132, -- 音量竖条浮层高度
}
-- ============================================================================
-- 样式区结束。下面是逻辑（命中判定/绘制/绑定），调外观不用往下看。
-- ============================================================================

local assdraw = require("mp.assdraw")

-- 播放状态（由 observe_property 回填，nil 表示还没拿到）
local st = {
    pause = true,
    pos = 0,
    dur = 0,
    vol = 50,
    mute = false,
}
local mouse = { x = -1, y = -1 } -- 最近一次鼠标位置（OSD 像素坐标）
local dragging = false -- 进度条拖拽中
local mouse_down = false
local fullscreen = false -- 窗口模式由 HTML PlayerBar 接管，原生 OSC 只服务全屏
local visible = false -- 全屏控制区交互时显示，空闲后自动隐藏
local hide_timer = nil
local HIDE_DELAY = 3000
local REVEAL_TOP_ZONE = 64
local REVEAL_BOTTOM_ZONE = 140
local last_drag_ratio = -1 -- 拖拽节流：变化很小就不重复 seek
local seek_armed = false -- 只有从进度条按下，后续移动才允许拖动 seek
local volume_armed = false -- 只有从音量竖条按下，后续移动才调整音量
local click_consumed = false -- 控件在 down 阶段已经完成动作，跳过延迟 click
local menu_kind = nil -- "sub" / "audio" / "volume"
local track_list = {}
local ui = {} -- 本次绘制的命中盒，由 render() 回填
local overlay = mp.create_osd_overlay("ass-events")

local function clamp(v, lo, hi)
    if v < lo then return lo end
    if v > hi then return hi end
    return v
end

local function inside(r, x, y)
    return r ~= nil and x >= r.x and x <= r.x + r.w and y >= r.y and y <= r.y + r.h
end

local function fmt_time(sec)
    if sec == nil or sec < 0 then return "--:--" end
    sec = math.floor(sec)
    local h = math.floor(sec / 3600)
    local m = math.floor((sec % 3600) / 60)
    local s = sec % 60
    if h > 0 then
        return string.format("%d:%02d:%02d", h, m, s)
    end
    return string.format("%02d:%02d", m, s)
end

-- 十进制 alpha(0-255) 转 ASS 的 &HXX& 片段
local function alpha_hex(a)
    return string.format("&H%02X&", clamp(math.floor(a or 0), 0, 255))
end

local render
local in_reveal_zone

local function ass_text(value)
    local text = tostring(value or "")
    text = text:gsub("\\", "")
    text = text:gsub("{", "(")
    return text:gsub("}", ")")
end

local function numeric_value(value, fallback)
    local number = tonumber(value)
    if number == nil then return fallback end
    return number
end

local function track_title(track, index, kind)
    local title = track.title or track.lang or track.codec
    if title == nil or tostring(title) == "" then
        local label = kind == "sub" and "字幕" or "音轨"
        title = label .. " " .. tostring(index)
    end
    if track.title ~= nil and track.lang ~= nil and tostring(track.lang) ~= "" then
        title = tostring(title) .. " · " .. tostring(track.lang)
    end
    return ass_text(title)
end

local function tracks_for(kind)
    local result = {}
    if type(track_list) ~= "table" then return result end
    for _, track in ipairs(track_list) do
        if type(track) == "table" and track.type == kind then
            result[#result + 1] = track
        end
    end
    return result
end

local function current_track_id(kind)
    if kind == "sub" then return st.sid end
    return st.aid
end

local function track_button_label(kind, fallback)
    local current = current_track_id(kind)
    if current == nil or current < 0 then return fallback .. "  ▾" end
    local tracks = tracks_for(kind)
    for index, track in ipairs(tracks) do
        if numeric_value(track.id, -1) == current then
            -- 顶部按钮只显示短语言/编码，完整名称放在下拉面板里，避免撑破按钮。
            local label = track.lang or track.codec
            if label ~= nil and tostring(label) ~= "" then
                return fallback .. " · " .. ass_text(label) .. "  ▾"
            end
            return fallback .. " " .. tostring(index) .. "  ▾"
        end
    end
    return fallback .. "  ▾"
end

local function popup_rect(x, y, w, h)
    return { x = x, y = y, w = w, h = h }
end

local function draw_popup(a, r)
    a:new_event()
    a:pos(0, 0)
    a:an(7)
    a:append("{\\blur8\\bord2\\1c&H" .. STYLE.col_panel .. "&\\1a"
        .. alpha_hex(STYLE.alpha_panel) .. "\\3c&H" .. STYLE.col_panel_border .. "&}")
    a:draw_start()
    a:round_rect_cw(r.x, r.y, r.x + r.w, r.y + r.h, STYLE.radius)
    a:draw_stop()
end

local function draw_popup_label(a, r, label, selected, disabled)
    a:new_event()
    a:pos(r.x + 12, r.y + 6)
    a:an(7)
    if disabled then
        a:append("{\\blur0\\bord0\\fn" .. STYLE.font .. "\\fs" .. STYLE.top_font_size
            .. "\\1c&H888888&}")
    elseif selected then
        a:append("{\\blur0\\bord0\\fn" .. STYLE.font .. "\\fs" .. STYLE.top_font_size
            .. "\\1c&H" .. STYLE.col_selected .. "&}")
    else
        a:append("{\\blur0\\bord0\\fn" .. STYLE.font .. "\\fs" .. STYLE.top_font_size
            .. "\\1c&H" .. STYLE.col_text .. "&}")
    end
    a:append(label)
end

local function set_volume_from_y(y)
    local slider = ui.volume_slider
    if slider == nil or slider.h <= 0 then return end
    local ratio = clamp(1 - ((y - slider.y) / slider.h), 0, 1)
    local volume = math.floor(ratio * 100 + 0.5)
    mp.commandv("set", "mute", "no")
    mp.commandv("set", "volume", tostring(volume))
    st.mute = false
    st.vol = volume
    render()
end

local function toggle_mute()
    mp.commandv("cycle", "mute")
    render()
end

local function close_menu()
    menu_kind = nil
    ui.sub_items = nil
    ui.audio_items = nil
    ui.volume_slider = nil
    ui.volume_mute = nil
end

local function menu_item_at(x, y)
    local items = menu_kind == "sub" and ui.sub_items or ui.audio_items
    if type(items) ~= "table" then return nil end
    for _, item in ipairs(items) do
        if inside(item.rect, x, y) then return item end
    end
    return nil
end

local function inside_open_popup(x, y)
    if menu_kind == "sub" then return inside(ui.sub_menu, x, y) end
    if menu_kind == "audio" then return inside(ui.audio_menu, x, y) end
    if menu_kind == "volume" then return inside(ui.volume_popup, x, y) end
    return false
end

local function in_overlay_interaction_zone(x, y)
    return in_reveal_zone(y) or inside_open_popup(x, y)
end

-- SEEK：按命中盒内的 x 算比例再绝对跳转
local function seek_to_x(x)
    local bar = ui.seek
    if bar == nil or bar.w <= 0 then return end
    local ratio = clamp((x - bar.x) / bar.w, 0, 1)
    if st.dur == nil or st.dur <= 0 then return end
    -- 拖拽节流：比例几乎没变就不发 seek，避免刷屏
    if dragging and math.abs(ratio - last_drag_ratio) < 0.002 then return end
    last_drag_ratio = ratio
    mp.commandv("seek", tostring(ratio * st.dur), "absolute")
end

render = function()
    if not visible then
        overlay.data = ""
        overlay:update()
        return
    end
    local w, h = mp.get_osd_size()
    if w == nil or w <= 0 or h == nil or h <= 0 then
        w, h = 1280, 720
    end
    local S = STYLE
    local a = assdraw.ass_new()

    -- ---------- 顶部：字幕 / 音轨（alignment 取顶 \an7） ----------
    local top_y = S.margin
    local sub_rect = { x = S.margin, y = top_y, w = S.top_btn_w, h = S.top_btn_h }
    local aud_rect = { x = S.margin + S.top_btn_w + 10, y = top_y, w = S.top_btn_w, h = S.top_btn_h }
    ui.sub, ui.audio = sub_rect, aud_rect
    -- 顶部背景条（半透明黑底，圆角）
    a:new_event()
    a:pos(0, 0)
    a:an(7)
    a:append("{\\blur0\\bord0\\1c&H" .. S.col_bg .. "&\\1a" .. alpha_hex(S.alpha_bg) .. "}")
    a:draw_start()
    if S.radius > 0 then
        a:round_rect_cw(S.margin - 6, top_y - 6,
            aud_rect.x + aud_rect.w + 6, top_y + S.top_btn_h + 6, S.radius)
    else
        a:rect_cw(S.margin - 6, top_y - 6,
            aud_rect.x + aud_rect.w + 6, top_y + S.top_btn_h + 6)
    end
    a:draw_stop()

    -- 顶部两个按钮：悬停描边高亮，其余保持纯文本
    local tops = {
        { r = sub_rect, label = track_button_label("sub", "SUB"), kind = "sub" },
        { r = aud_rect, label = track_button_label("audio", "AUD"), kind = "audio" },
    }
    for _, b in ipairs(tops) do
        local hovered = inside(b.r, mouse.x, mouse.y)
        a:new_event()
        a:pos(b.r.x, b.r.y)
        a:an(7) -- 取顶：以按钮左上角为锚点
        if hovered then
            a:append("{\\blur0\\bord1\\fn" .. S.font .. "\\fs" .. S.top_font_size
                .. "\\1c&H" .. S.col_text .. "&\\3c&H" .. S.col_hover .. "&}")
        else
            a:append("{\\blur0\\bord0\\fn" .. S.font .. "\\fs" .. S.top_font_size
                .. "\\1c&H" .. S.col_text .. "&}")
        end
        a:append(b.label)
    end

    -- ---------- 顶部：字幕/音轨下拉面板 ----------
    ui.sub_menu, ui.audio_menu = nil, nil
    ui.sub_items, ui.audio_items = nil, nil
    local function draw_track_menu(kind, anchor)
        local tracks = tracks_for(kind)
        local items = {
            { label = kind == "sub" and "关闭字幕" or "关闭音轨", id = "no", disabled = false },
        }
        for index, track in ipairs(tracks) do
            items[#items + 1] = {
                label = track_title(track, index, kind),
                id = numeric_value(track.id, -1),
                disabled = false,
            }
        end
        if #items == 1 then
            items[#items + 1] = {
                label = kind == "sub" and "暂无字幕" or "暂无音轨",
                id = nil,
                disabled = true,
            }
        end

        local max_items = 8
        local visible_items = math.min(#items, max_items)
        local panel = popup_rect(anchor.x, anchor.y + anchor.h + 8, S.menu_w,
            visible_items * S.menu_row_h + 8)
        if kind == "sub" then ui.sub_menu = panel else ui.audio_menu = panel end
        draw_popup(a, panel)

        local hit_items = {}
        for index = 1, visible_items do
            local item = items[index]
            local rect = popup_rect(panel.x + 4, panel.y + 4 + (index - 1) * S.menu_row_h,
                panel.w - 8, S.menu_row_h)
            local selected = false
            if item.id == "no" then
                selected = current_track_id(kind) == nil or current_track_id(kind) < 0
            elseif item.id ~= nil then
                selected = current_track_id(kind) == item.id
            end
            local hovered = inside(rect, mouse.x, mouse.y)
            if hovered and not item.disabled then
                a:new_event()
                a:pos(0, 0)
                a:an(7)
                a:append("{\\blur0\\bord0\\1c&H" .. S.col_selected .. "&\\1a&H90&}")
                a:draw_start()
                a:round_rect_cw(rect.x, rect.y, rect.x + rect.w, rect.y + rect.h, 5)
                a:draw_stop()
            end
            draw_popup_label(a, rect, item.label, selected, item.disabled)
            hit_items[#hit_items + 1] = { rect = rect, id = item.id, disabled = item.disabled, kind = kind }
        end
        if kind == "sub" then ui.sub_items = hit_items else ui.audio_items = hit_items end
    end

    if menu_kind == "sub" then
        draw_track_menu("sub", sub_rect)
    elseif menu_kind == "audio" then
        draw_track_menu("audio", aud_rect)
    end

    -- ---------- 底部：背景条 ----------
    local bar_y = h - S.margin - S.bottom_h
    ui.bar = { x = S.margin, y = bar_y, w = w - 2 * S.margin, h = S.bottom_h }
    a:new_event()
    a:pos(0, 0)
    a:an(1) -- 取底：坐标按底部条理解（盒子仍用绝对坐标画）
    a:append("{\\blur0\\bord0\\1c&H" .. S.col_bg .. "&\\1a" .. alpha_hex(S.alpha_bottom_bg) .. "}")
    a:draw_start()
    if S.radius > 0 then
        a:round_rect_cw(ui.bar.x, ui.bar.y, ui.bar.x + ui.bar.w, ui.bar.y + ui.bar.h, S.radius)
    else
        a:rect_cw(ui.bar.x, ui.bar.y, ui.bar.x + ui.bar.w, ui.bar.y + ui.bar.h)
    end
    a:draw_stop()

    -- ---------- 底部：进度条（点击+拖拽） ----------
    local seek_x = ui.bar.x + S.gap
    local seek_w = ui.bar.w - 2 * S.gap
    local seek_y = bar_y + S.gap
    -- 命中区比可见进度条上下各扩 12px，降低点击精度要求；必须与
    -- win32.rs 的 OSC_SEEK_HIT_PAD_PX 保持同步。
    local seek_hit_pad = 12
    ui.seek = { x = seek_x, y = seek_y - seek_hit_pad, w = seek_w, h = S.seek_h + 2 * seek_hit_pad }
    local ratio = 0
    if st.dur ~= nil and st.dur > 0 and st.pos ~= nil and st.pos > 0 then
        ratio = clamp(st.pos / st.dur, 0, 1)
    end
    local fill_w = math.floor(seek_w * ratio)
    a:new_event()
    a:pos(0, 0)
    a:an(1)
    a:append("{\\blur0\\bord0\\1c&H" .. S.col_bar_bg .. "&\\1a" .. alpha_hex(S.alpha_bar) .. "}")
    a:draw_start()
    a:rect_cw(seek_x, seek_y, seek_x + seek_w, seek_y + S.seek_h)
    a:draw_stop()
    if fill_w > 0 then
        a:new_event()
        a:pos(0, 0)
        a:an(1)
        a:append("{\\blur0\\bord0\\1c&H" .. S.col_bar_fill .. "&\\1a" .. alpha_hex(0) .. "}")
        a:draw_start()
        a:rect_cw(seek_x, seek_y, seek_x + fill_w, seek_y + S.seek_h)
        a:draw_stop()
    end

    -- ---------- 底部：播放/时间/音量（alignment 取底 \an1） ----------
    local row_y = seek_y + S.seek_h + 8 -- 按钮行顶边
    local row_h = bar_y + S.bottom_h - row_y - 8
    local cx = ui.bar.x + S.gap
    local play_rect = { x = cx, y = row_y, w = S.play_btn_w, h = row_h }
    ui.play = play_rect
    cx = cx + S.play_btn_w + S.gap
    local vol_rect = { x = ui.bar.x + ui.bar.w - S.gap - S.vol_btn_w, y = row_y, w = S.vol_btn_w, h = row_h }
    ui.vol = vol_rect
    local time_rect = { x = vol_rect.x - S.gap - S.time_label_w, y = row_y, w = S.time_label_w, h = row_h }
    ui.time = time_rect
    ui.volume_popup, ui.volume_slider, ui.volume_mute = nil, nil, nil

    local play_label = "PAUSE"
    if st.pause then play_label = "PLAY" end
    local vol_label
    if st.mute then
        vol_label = "MUTE"
    else
        vol_label = "VOL " .. tostring(math.floor(st.vol or 0))
    end
    local time_label = fmt_time(st.pos) .. " / " .. fmt_time(st.dur)

    local bottoms = {
        { r = play_rect, label = play_label },
        { r = time_rect, label = time_label },
        { r = vol_rect, label = vol_label },
    }
    for _, b in ipairs(bottoms) do
        local hovered = inside(b.r, mouse.x, mouse.y)
        a:new_event()
        a:pos(b.r.x, b.r.y + b.r.h) -- \an1 锚点在左下角
        a:an(1) -- 取底：以左下角为锚点
        if hovered then
            a:append("{\\blur0\\bord1\\fn" .. S.font .. "\\fs" .. S.font_size
                .. "\\1c&H" .. S.col_text .. "&\\3c&H" .. S.col_hover .. "&}")
        else
            a:append("{\\blur0\\bord0\\fn" .. S.font .. "\\fs" .. S.font_size
                .. "\\1c&H" .. S.col_text .. "&}")
        end
        a:append(b.label)
    end

    -- ---------- 音量：点击按钮后显示竖向滑杆 ----------
    if menu_kind == "volume" then
        local popup = popup_rect(vol_rect.x + vol_rect.w - S.volume_popup_w,
            vol_rect.y - S.volume_popup_h - 8, S.volume_popup_w, S.volume_popup_h)
        local slider = popup_rect(popup.x + 20, popup.y + 14, 18, popup.h - 50)
        local mute_rect = popup_rect(popup.x + 6, popup.y + popup.h - 30, popup.w - 12, 22)
        ui.volume_popup = popup
        ui.volume_slider = slider
        ui.volume_mute = mute_rect
        draw_popup(a, popup)

        local slider_track = popup_rect(slider.x + 5, slider.y, 8, slider.h)
        a:new_event()
        a:pos(0, 0)
        a:an(7)
        a:append("{\\blur0\\bord0\\1c&H555555&\\1a&H30&}")
        a:draw_start()
        a:round_rect_cw(slider_track.x, slider_track.y,
            slider_track.x + slider_track.w, slider_track.y + slider_track.h, 4)
        a:draw_stop()

        local volume_ratio = clamp(numeric_value(st.vol, 0) / 100, 0, 1)
        local fill_h = math.max(2, math.floor(slider_track.h * volume_ratio))
        local fill_y = slider_track.y + slider_track.h - fill_h
        a:new_event()
        a:pos(0, 0)
        a:an(7)
        a:append("{\\blur0\\bord0\\1c&H" .. S.col_selected .. "&}")
        a:draw_start()
        a:round_rect_cw(slider_track.x, fill_y,
            slider_track.x + slider_track.w, slider_track.y + slider_track.h, 4)
        a:draw_stop()

        local knob_y = clamp(fill_y - 4, slider_track.y - 4, slider_track.y + slider_track.h - 4)
        a:new_event()
        a:pos(0, 0)
        a:an(7)
        a:append("{\\blur0\\bord0\\1c&HFFFFFF&}")
        a:draw_start()
        a:round_rect_cw(slider_track.x - 5, knob_y, slider_track.x + slider_track.w + 5, knob_y + 8, 4)
        a:draw_stop()

        local mute_hover = inside(mute_rect, mouse.x, mouse.y)
        a:new_event()
        a:pos(mute_rect.x + mute_rect.w / 2, mute_rect.y + 4)
        a:an(8)
        if mute_hover then
            a:append("{\\blur0\\bord0\\fn" .. S.font .. "\\fs14\\1c&H" .. S.col_hover .. "&}")
        else
            a:append("{\\blur0\\bord0\\fn" .. S.font .. "\\fs14\\1c&H" .. S.col_text .. "&}")
        end
        a:append(st.mute and "取消静音" or "静音")
    end

    overlay.res_x = w
    overlay.res_y = h
    overlay.data = a.text
    overlay.z = 1000
    overlay:update()
end

local function force_hide_overlay()
    if hide_timer ~= nil then
        hide_timer:kill()
        hide_timer = nil
    end
    close_menu()
    visible = false
    render()
end

local function hide_overlay()
    if mouse_down or dragging then return end
    if not visible and hide_timer == nil then return end
    force_hide_overlay()
end

local function schedule_hide()
    if not fullscreen or mouse_down or dragging then return end
    if hide_timer ~= nil then
        hide_timer:kill()
        hide_timer = nil
    end
    hide_timer = mp.add_timeout(HIDE_DELAY / 1000, function()
        hide_timer = nil
        hide_overlay()
    end)
end

local function show_overlay()
    if not fullscreen then return end
    visible = true
    if hide_timer ~= nil then
        hide_timer:kill()
        hide_timer = nil
    end
    schedule_hide()
    render()
end

in_reveal_zone = function(y)
    local _, h = mp.get_osd_size()
    if h == nil or h <= 0 then return false end
    return y <= REVEAL_TOP_ZONE or y >= h - REVEAL_BOTTOM_ZONE
end

local function update_mouse(x, y)
    local next_x = tonumber(x)
    local next_y = tonumber(y)
    if next_x ~= nil then mouse.x = next_x end
    if next_y ~= nil then mouse.y = next_y end
end

local function toggle_menu(kind)
    if menu_kind == kind then
        close_menu()
    else
        close_menu()
        menu_kind = kind
    end
    render()
end

local function select_track(item)
    if item == nil or item.disabled or item.id == nil then return end
    if item.kind == "sub" then
        mp.commandv("set", "sid", tostring(item.id))
    else
        mp.commandv("set", "aid", tostring(item.id))
    end
    close_menu()
    render()
end

local function handle_control_down()
    if not fullscreen then return false end

    local item = menu_item_at(mouse.x, mouse.y)
    if item ~= nil then
        select_track(item)
        click_consumed = true
        return true
    end

    if menu_kind == "volume" then
        if inside(ui.volume_slider, mouse.x, mouse.y) then
            volume_armed = true
            click_consumed = true
            set_volume_from_y(mouse.y)
            return true
        end
        if inside(ui.volume_mute, mouse.x, mouse.y) then
            click_consumed = true
            toggle_mute()
            return true
        end
    end

    if inside(ui.sub, mouse.x, mouse.y) then
        toggle_menu("sub")
        click_consumed = true
        return true
    end
    if inside(ui.audio, mouse.x, mouse.y) then
        toggle_menu("audio")
        click_consumed = true
        return true
    end
    if inside(ui.vol, mouse.x, mouse.y) then
        toggle_menu("volume")
        click_consumed = true
        return true
    end

    if menu_kind ~= nil and not inside_open_popup(mouse.x, mouse.y) then
        close_menu()
        render()
        click_consumed = true
        return true
    end
    return false
end

local function handle_single_click()
    if click_consumed then
        click_consumed = false
        return
    end
    if inside(ui.play, mouse.x, mouse.y) then
        mp.commandv("cycle", "pause")
        return
    end
    if inside(ui.sub, mouse.x, mouse.y) then
        toggle_menu("sub")
        return
    end
    if inside(ui.audio, mouse.x, mouse.y) then
        toggle_menu("audio")
        return
    end
    if inside(ui.vol, mouse.x, mouse.y) then
        toggle_menu("volume")
        return
    end
    local item = menu_item_at(mouse.x, mouse.y)
    if item ~= nil then
        select_track(item)
        return
    end
    if inside(ui.volume_slider, mouse.x, mouse.y) then
        set_volume_from_y(mouse.y)
        return
    end
    if inside(ui.volume_mute, mouse.x, mouse.y) then
        toggle_mute()
        return
    end
    if inside(ui.seek, mouse.x, mouse.y) then
        seek_to_x(mouse.x)
    end
end

local function handle_right_click()
    -- 右键只走自家菜单：与左键同一数据源（track-list observer），永不弹原生菜单。
    if inside(ui.sub, mouse.x, mouse.y) then
        toggle_menu("sub")
        return
    end
    if inside(ui.audio, mouse.x, mouse.y) then
        toggle_menu("audio")
    end
end

local function handle_surface_mouse(phase, x, y)
    update_mouse(x, y)
    if phase == "move" then
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        elseif fullscreen then
            -- 离开顶部/底部唤出区后立即收起，避免控制条常驻在画面上。
            hide_overlay()
        end
    elseif phase == "drag" then
        if fullscreen and volume_armed then
            show_overlay()
            set_volume_from_y(mouse.y)
        elseif fullscreen and seek_armed then
            show_overlay()
            dragging = true
            seek_to_x(mouse.x)
        end
    elseif phase == "down" then
        mouse_down = true
        click_consumed = false
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        end
        local control_down = handle_control_down()
        if control_down then
            seek_armed = false
        else
            seek_armed = fullscreen and inside(ui.seek, mouse.x, mouse.y)
            if seek_armed then last_drag_ratio = -1 end
        end
    elseif phase == "up" then
        if volume_armed and fullscreen then
            set_volume_from_y(mouse.y)
        elseif dragging and fullscreen and seek_armed then
            seek_to_x(mouse.x)
        end
        dragging = false
        seek_armed = false
        volume_armed = false
        mouse_down = false
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        elseif fullscreen then
            hide_overlay()
        else
            schedule_hide()
        end
    elseif phase == "seek-click" then
        -- Timeline clicks are immediate and never participate in the
        -- single/double-click timer used by the video surface.
        if fullscreen then
            show_overlay()
            seek_to_x(mouse.x)
        end
        seek_armed = false
        volume_armed = false
        click_consumed = false
    elseif phase == "click" then
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        elseif fullscreen then
            hide_overlay()
        end
        if fullscreen then handle_single_click() end
    elseif phase == "double" then
        -- Win32 already canceled the pending single click. A double click
        -- never invokes an OSC single-click action.
        click_consumed = false
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        elseif fullscreen then
            hide_overlay()
        end
    elseif phase == "right-down" then
        mouse_down = true
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        end
        if fullscreen then handle_right_click() end
    elseif phase == "right-up" then
        mouse_down = false
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        elseif fullscreen then
            hide_overlay()
        else
            schedule_hide()
        end
    elseif phase == "wheel-up" then
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        end
        if fullscreen and (inside(ui.vol, mouse.x, mouse.y) or inside(ui.volume_slider, mouse.x, mouse.y)) then
            mp.commandv("add", "volume", "5")
        end
    elseif phase == "wheel-down" then
        if fullscreen and in_overlay_interaction_zone(mouse.x, mouse.y) then
            show_overlay()
        end
        if fullscreen and (inside(ui.vol, mouse.x, mouse.y) or inside(ui.volume_slider, mouse.x, mouse.y)) then
            mp.commandv("add", "volume", "-5")
        end
    elseif phase == "leave" then
        if not mouse_down and not dragging then force_hide_overlay() end
    elseif phase == "cancel" then
        mouse_down = false
        dragging = false
        seek_armed = false
        volume_armed = false
        click_consumed = false
        close_menu()
        force_hide_overlay()
    end
end

local function handle_surface_mode(mode)
    fullscreen = mode == "fullscreen"
    if not fullscreen then
        mouse_down = false
        dragging = false
        seek_armed = false
        volume_armed = false
        click_consumed = false
        close_menu()
        force_hide_overlay()
    end
end

-- ---------- 状态订阅（只读展示用，不改播放逻辑） ----------
mp.observe_property("pause", "bool", function(_, v)
    if v ~= nil then st.pause = v end
    render()
end)
mp.observe_property("time-pos", "number", function(_, v)
    if v ~= nil then st.pos = v end
    render()
end)
mp.observe_property("duration", "number", function(_, v)
    if v ~= nil then st.dur = v end
    render()
end)
mp.observe_property("volume", "number", function(_, v)
    if v ~= nil then st.vol = v end
    render()
end)
mp.observe_property("mute", "bool", function(_, v)
    if v ~= nil then st.mute = v end
    render()
end)
-- 字幕/音轨/轨道列表变化时重绘，并刷新下拉面板的选中态。
mp.observe_property("sid", "native", function(_, v)
    st.sid = numeric_value(v, -1)
    render()
end)
mp.observe_property("aid", "native", function(_, v)
    st.aid = numeric_value(v, -1)
    render()
end)
mp.observe_property("track-list", "native", function(_, v)
    if type(v) == "table" then
        track_list = v
    else
        track_list = {}
    end
    render()
end)
mp.observe_property("osd-dimensions", "native", function() render() end)
mp.register_script_message("lumina-surface-mouse", handle_surface_mouse)
mp.register_script_message("lumina-surface-mode", handle_surface_mode)

-- 没播东西时也先生成一帧，但保持隐藏；首次移动/进入时显示。
render()

mp.msg.info("lumina-osc skin loaded")
