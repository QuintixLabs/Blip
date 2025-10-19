#Requires AutoHotkey v2.0

;==============================================================
; Blip — Quintix
;==============================================================
; Copyright (c) 2025 Quintix
; Licensed under the GNU General Public License v3.0
; License: https://github.com/QuintixLabs/Blip/blob/master/LICENSE
;
; Issues:      https://github.com/QuintixLabs/Blip/issues
; Contributions: https://github.com/QuintixLabs/Blip/pulls
;
; You may use, modify, and redistribute under GPLv3.
; Do not sell or claim this work as your own.
;==============================================================


; Variables
global initialSize := 15
global maxSize := 40
global shadowColor := "000000"
global maxOpacity := 60
global expandSpeed := 6
global fadeStartPercent := 0.5
global isCheckingForUpdates := false
global settingsGui := ""
global blockIBeam := true
global blockHand := false
global blockTaskbar := true
global runOnStartup := true
global shadowShape := "Circle"
global blockGames := true

; Version Variables
global currentVersion := "1.0"
global versionCheckUrl := "https://itsblip.netlify.app/version/version.txt"

; Save settings in %LOCALAPPDATA%
configDir := EnvGet("LOCALAPPDATA") "\..\Local\Blip"
configFile := configDir . "\settings.ini"
DirCreate(configDir)

if FileExist(configFile) {
    initialSize := IniRead(configFile, "Settings", "InitialSize", 15)
    maxSize := IniRead(configFile, "Settings", "MaxSize", 50)
    shadowColor := IniRead(configFile, "Settings", "ShadowColor", "000000")
    maxOpacity := IniRead(configFile, "Settings", "MaxOpacity", 80)
    expandSpeed := IniRead(configFile, "Settings", "ExpandSpeed", 8)
    fadeStartPercent := Float(IniRead(configFile, "Settings", "FadeStartPercent", 0.6))
    blockIBeam := IniRead(configFile, "Settings", "BlockIBeam", 1)
    blockHand := IniRead(configFile, "Settings", "BlockHand", 0)
    blockTaskbar := IniRead(configFile, "Settings", "BlockTaskbar", 1)
    runOnStartup := IniRead(configFile, "Settings", "RunOnStartup", 0)
    shadowShape := IniRead(configFile, "Settings", "ShadowShape", "Circle")
    blockGames := IniRead(configFile, "Settings", "BlockGames", 1)
}

; Track active shadows and click state
shadows := []
clickStartX := 0
clickStartY := 0
clickStartTime := 0
shouldBlockShadow := false

; Pre-create a pool of shadow GUIs
shadowPool := []
poolSize := 20

Loop poolSize {
    shadow := Gui("+AlwaysOnTop -Caption +ToolWindow +E0x20")
    shadow.BackColor := shadowColor
    shadowPool.Push({gui: shadow, inUse: false})
}

; System Tray
InitializeTrayMenu() {
    A_IconTip := "Blip"
    A_TrayMenu.Delete()
    A_TrayMenu.Add("Settings", OpenSettings)
    A_TrayMenu.Add("Exit", ExitScript)
    OnMessage(0x404, TrayMessage)

    TrayMessage(wParam, lParam, *) {
        if (lParam = 0x202) { ; WM_LBUTTONUP = left-click
            OpenSettings()
        }
    }
}

InitializeTrayMenu()

; Dark mode for System Tray
class darkMode {
    static __New(Mode := 1) => (
        DllCall(DllCall("GetProcAddress", "ptr", DllCall("GetModuleHandle", "str", "uxtheme", "ptr"), "ptr", 135, "ptr"), "int", Mode),
        DllCall(DllCall("GetProcAddress", "ptr", DllCall("GetModuleHandle", "str", "uxtheme", "ptr"), "ptr", 136, "ptr"))
    )
}

darkMode()

SetTimer(() => CheckForUpdates(false), -2000)
SetTimer(CheckMouseState, 5)

; CheckMouseState Function
CheckMouseState() {
    global clickStartX, clickStartY, clickStartTime, shouldBlockShadow, blockIBeam, blockHand, blockGames
    static wasPressed := false
    static clickProcessed := false
    static mouseMovedWhilePressed := false
    
    isPressed := GetKeyState("LButton", "P")
    
    if (isPressed && !wasPressed) {
        CoordMode("Mouse", "Screen")
        MouseGetPos(&clickStartX, &clickStartY)
        clickStartTime := A_TickCount
        
        currentCursor := A_Cursor
        shouldBlockShadow := (blockIBeam && currentCursor == "IBeam") || (blockHand && currentCursor == "Unknown")
        
        if (blockGames) {
            try {
                clipRect := Buffer(16, 0)
                DllCall("GetClipCursor", "Ptr", clipRect)
                
                left := NumGet(clipRect, 0, "Int")
                top := NumGet(clipRect, 4, "Int")
                right := NumGet(clipRect, 8, "Int")
                bottom := NumGet(clipRect, 12, "Int")
                
                clipWidth := right - left
                clipHeight := bottom - top
                
                if (clipWidth > 0 && clipHeight > 0 && (clipWidth < A_ScreenWidth || clipHeight < A_ScreenHeight)) {
                    shouldBlockShadow := true
                }
            }
        }
        
        mouseMovedWhilePressed := false
        clickProcessed := false
        wasPressed := true
    }
    else if (isPressed && wasPressed) {
        if ((A_TickCount - clickStartTime) > 20) {
            CoordMode("Mouse", "Screen")
            MouseGetPos(&currentX, &currentY)
            distance := Sqrt((currentX - clickStartX)**2 + (currentY - clickStartY)**2)
            
            if (distance > 100)
                mouseMovedWhilePressed := true
        }
    }
    else if (!isPressed && wasPressed) {
        if (!clickProcessed) {
            CoordMode("Mouse", "Screen")
            MouseGetPos(&releaseX, &releaseY)
           
            if (blockTaskbar) {
                hwnd := DllCall("WindowFromPoint", "Int64", (releaseY << 32) | (releaseX & 0xFFFFFFFF), "Ptr")
                try {
                    className := WinGetClass("ahk_id " hwnd)
                    if (className ~= "i)(Shell_TrayWnd|MSTaskSwWClass|ReBarWindow32|TrayNotifyWnd|Shell_SecondaryTrayWnd)") {
                        clickProcessed := true
                        wasPressed := false
                        return
                    }
                    
                    parentHwnd := hwnd
                    Loop 5 {
                        parentHwnd := DllCall("GetParent", "Ptr", parentHwnd, "Ptr")
                        if (!parentHwnd)
                            break
                        try {
                            parentClass := WinGetClass("ahk_id " parentHwnd)
                            if (parentClass ~= "i)(Shell_TrayWnd)") {
                                clickProcessed := true
                                wasPressed := false
                                return
                            }
                        }
                    }
                }
            }
           
            if (!shouldBlockShadow && !mouseMovedWhilePressed) {
                SetTimer(() => CreateShadowFromPool(releaseX, releaseY), -15)
            }
           
            clickProcessed := true
        }
        wasPressed := false
    }
}

; CreateShadowFromPool Function
CreateShadowFromPool(x, y) {
    global shadows, shadowPool, initialSize, shadowColor, maxOpacity, shadowShape
    
    shadowObj := ""
    for poolItem in shadowPool {
        if (!poolItem.inUse) {
            shadowObj := poolItem
            break
        }
    }
    
    if (shadowObj == "") {
        return
    }
    
    shadowObj.inUse := true
    
    CoordMode("Mouse", "Screen")
    
    shadowObj.gui.BackColor := shadowColor
    
    posX := x - initialSize/2
    posY := y - initialSize/2
    
    shadowObj.gui.Show("x" posX " y" posY " w" initialSize " h" initialSize " NA")
    SetShapeRegion(shadowObj.gui, initialSize, shadowShape)
    
    activeObj := {
        poolItem: shadowObj,
        size: initialSize,
        opacity: maxOpacity,
        centerX: x,
        centerY: y,
        progress: 0
    }
    
    shadows.Push(activeObj)
    
    if (shadows.Length = 1)
        SetTimer(AnimateShadows, 5)
}

; Shapes
SetShapeRegion(guiObj, size, shape) {
    switch shape {
        case "Circle":
            WinSetRegion("0-0 W" size " H" size " E", guiObj)
        case "Square":
            WinSetRegion("0-0 W" size " H" size, guiObj)
        case "Hexagon":
            CreateHexagonRegion(guiObj, size)
        case "Diamond":
            CreateDiamondRegion(guiObj, size)
    }
}

; Create sum special shapes cuz yeah :D
CreateHexagonRegion(guiObj, size) {
    centerX := size / 2
    centerY := size / 2
    radius := size / 2
    
    points := []
    Loop 6 {
        angle := (A_Index - 1) * 60 * 3.14159265359 / 180
        x := Round(centerX + radius * Cos(angle))
        y := Round(centerY + radius * Sin(angle))
        points.Push(x "-" y)
    }
    
    region := points[1]
    Loop points.Length - 1
        region .= " " points[A_Index + 1]
    
    WinSetRegion(region, guiObj)
}

CreateDiamondRegion(guiObj, size) {
    centerX := size / 2
    centerY := size / 2
    halfSize := size / 2
    
    region := centerX "-0 " size "-" centerY " " centerX "-" size " 0-" centerY
    WinSetRegion(region, guiObj)
}

; animate the shadows
AnimateShadows() {
    global shadows, maxSize, expandSpeed, fadeStartPercent, maxOpacity, initialSize, shadowShape
    
    i := 1
    while (i <= shadows.Length) {
        shadow := shadows[i]
        
        speed := Max(1, expandSpeed)
        
        shadow.progress += speed
        progress_normalized := shadow.progress / 100
        if (progress_normalized > 1)
            progress_normalized := 1
        
        eased := 1 - ((1 - progress_normalized) ** 3)
        newSize := initialSize + ((maxSize - initialSize) * eased)
        
        if (progress_normalized > fadeStartPercent) {
            fade_progress := (progress_normalized - fadeStartPercent) / (1 - fadeStartPercent)
            shadow.opacity := maxOpacity * (1 - fade_progress)
        }
        
        shadow.opacity := Max(0, Min(255, shadow.opacity))
        
        if (progress_normalized >= 1) {
            shadow.poolItem.gui.Hide()
            shadow.poolItem.inUse := false
            shadows.RemoveAt(i)
            continue
        }
        
        shadow.size := newSize
        newX := shadow.centerX - newSize/2
        newY := shadow.centerY - newSize/2
        
        shadow.poolItem.gui.Move(newX, newY, newSize, newSize)
        SetShapeRegion(shadow.poolItem.gui, Integer(newSize), shadowShape)
        WinSetTransparent(Integer(shadow.opacity), shadow.poolItem.gui)
        
        i++
    }
    
    if (shadows.Length = 0)
        SetTimer(AnimateShadows, 0)
}

;==================================================
;                    GUI SECTION
;==================================================

; Proper MsgBox handling function
ShowMsgBox(text, title, options := "OK") {
    global settingsGui
    
    ; Check if GUI object exists and is valid
    guiExists := (IsObject(settingsGui) && settingsGui != "" && WinExist("Blip ahk_id " settingsGui.Hwnd))
    
    if (guiExists) {
        ; REMOVE AlwaysOnTop so MsgBox can appear above it
        WinSetAlwaysOnTop(0, "Blip ahk_id " settingsGui.Hwnd)
    }
    
    ; Show MsgBox
    result := MsgBox(text, title, options)
    
    if (guiExists) {
        ; Restore AlwaysOnTop after MsgBox closes
        WinSetAlwaysOnTop(1, "Blip ahk_id " settingsGui.Hwnd)
        ; Re-activate the GUI
        WinActivate("Blip ahk_id " settingsGui.Hwnd)
    }
    
    return result
}

OpenSettings(*) {
    global initialSize, maxSize, shadowColor, maxOpacity, expandSpeed, fadeStartPercent, configFile, currentVersion, settingsGui, blockIBeam, blockHand, runOnStartup, shadowShape, blockGames
    
    if (settingsGui != "" && WinExist("Blip ahk_id " settingsGui.Hwnd)) {
        WinActivate("Blip ahk_id " settingsGui.Hwnd)
        return
    }
    
    settingsGui := Gui("+AlwaysOnTop", "Blip")  ; Added AlwaysOnTop
    settingsGui.BackColor := "1A1A1A"
    DllCall("uxtheme\SetWindowTheme", "Ptr", settingsGui.Hwnd, "Str", "DarkMode_Explorer", "Ptr", 0)
    DllCall("dwmapi\DwmSetWindowAttribute", "Ptr", settingsGui.Hwnd, "Int", 20, "Int*", true, "Int", 4)
    settingsGui.SetFont("s10 cWhite", "Segoe UI")
    
    ; === Shadow Settings Section ===
    settingsGui.SetFont("s11 cWhite Bold")
    settingsGui.Add("Text", "x20 y35 cWhite", "Shadow Settings")
    settingsGui.SetFont("s10 cWhite Norm")
    
    ; Shadow Shape
    settingsGui.Add("Text", "x20 y82 cWhite", "Shadow Shape:")
    shapeDropDown := settingsGui.Add("DropDownList", "x140 y78 w100 h200 cBlack BackgroundFFFFFF Choose" GetShapeIndex(shadowShape), ["Circle", "Square", "Hexagon", "Diamond"])
    
    ; Shape Preview
    shapePreviewBg := settingsGui.Add("Picture", "x250 y65 w50 h50 Background2b2b2b")
    
    global shapePreviewCtrls := []
    global shapePreviewCtrl := settingsGui.Add("Text", "x250 y65 w50 h50 Background" shadowColor)
    shapePreviewCtrls.Push(shapePreviewCtrl)
    global currentSettingsGui := settingsGui
    
    SetShapeRegion(shapePreviewCtrl, 50, shadowShape)
    WinSetTransparent(maxOpacity, shapePreviewCtrl)
    
    shapeDropDown.OnEvent("Change", (*) => RecreateShapePreview(shapeDropDown, maxOpacityEdit))
    
    ; Shadow Color
    settingsGui.Add("Text", "x20 y130 cWhite", "Shadow Color:")
    colorPreview := settingsGui.Add("Edit", "x140 y130 w100 h25 cBlack BackgroundFFFFFF", shadowColor)
    colorPreview.OnEvent("Change", (*) => UpdateColorFromHex(colorPreview, colorPreviewBox, maxOpacityEdit, shapeDropDown))
    colorPreviewBox := settingsGui.Add("Text", "x250 y130 w25 h25 Background" shadowColor)
    colorPreviewBox.Redraw()
    
    chooseBtn := CreateTextButton(settingsGui, "Choose Color", 280, 130, 100, 25, (*) => ChooseColor(colorPreview, colorPreviewBox, shapePreviewCtrl, maxOpacityEdit, shapeDropDown))
    
    ; Initial Size
    settingsGui.Add("Text", "x20 y170 cWhite", "Initial Size:")
    initialSizeEdit := settingsGui.Add("Edit", "x140 y170 w100 h25 cBlack BackgroundFFFFFF", initialSize)
    
    ; Max Size
    settingsGui.Add("Text", "x20 y210 cWhite", "Max Size:")
    maxSizeEdit := settingsGui.Add("Edit", "x140 y210 w100 h25 cBlack BackgroundFFFFFF", maxSize)
    
    ; Max Opacity
    settingsGui.Add("Text", "x20 y250 cWhite", "Max Opacity:")
    maxOpacityEdit := settingsGui.Add("Edit", "x140 y250 w100 h25 cBlack BackgroundFFFFFF", maxOpacity)
    maxOpacityEdit.OnEvent("Change", (*) => UpdatePreviewOpacity(maxOpacityEdit))
    
    ; Expand Speed
    settingsGui.Add("Text", "x20 y290 cWhite", "Expand Speed:")
    expandSpeedEdit := settingsGui.Add("Edit", "x140 y290 w100 h25 cBlack BackgroundFFFFFF", expandSpeed)
    
    ; Fade Start Percent
    settingsGui.Add("Text", "x20 y330 cWhite", "Fade Start %:")
    fadeStartPercentEdit := settingsGui.Add("Edit", "x140 y330 w100 h25 cBlack BackgroundFFFFFF", Format("{:.2f}", fadeStartPercent))
    
    ; Separator line
    settingsGui.Add("Text", "x20 y370 w360 h1 Background555555")
    
    ; === Cursor Blocking Section ===
    settingsGui.SetFont("s11 cWhite Bold")
    settingsGui.Add("Text", "x20 y380 cWhite", "Shadow Blocking")
    settingsGui.SetFont("s10 cWhite Norm")
    
    blockIBeamCheck := settingsGui.Add("CheckBox", "x20 y410 cWhite", "Block on Text Cursor (I-beam)")
    blockIBeamCheck.Value := blockIBeam
    
    blockHandCheck := settingsGui.Add("CheckBox", "x20 y440 cWhite", "Block on Link Cursor (Hand)")
    blockHandCheck.Value := blockHand

    blockTaskbarCheck := settingsGui.Add("CheckBox", "x20 y470 cWhite", "Block shadows on Taskbar")
    blockTaskbarCheck.Value := blockTaskbar
    
    blockGamesCheck := settingsGui.Add("CheckBox", "x20 y500 cWhite", "Block shadows in Games")
    blockGamesCheck.Value := blockGames
    
    ; Separator line
    settingsGui.Add("Text", "x20 y540 w360 h1 Background555555")
    
    ; === General Settings Section ===
    settingsGui.SetFont("s11 cWhite Bold")
    settingsGui.Add("Text", "x20 y550 cWhite", "General")
    settingsGui.SetFont("s10 cWhite Norm")
    
    runOnStartupCheck := settingsGui.Add("CheckBox", "x20 y580 cWhite", "Run on Windows startup")
    runOnStartupCheck.Value := runOnStartup
    
    ; Separator line
    settingsGui.Add("Text", "x20 y620 w360 h1 Background555555")
    
    ; === About Section ===
    settingsGui.SetFont("s11 cWhite Bold")
    settingsGui.Add("Text", "x20 y630 cWhite", "About")
    settingsGui.SetFont("s10 cWhite Norm")
    
    settingsGui.Add("Text", "x20 y660 cWhite", "Version: v" currentVersion)
    
    websiteText := settingsGui.Add("Text", "x20 y685 w60 h20 c4FC4FF BackgroundTrans", "Website")
    websiteText.SetFont("s10 underline")
    websiteText.OnEvent("Click", (*) => OpenUrl("https://itsblip.netlify.app", settingsGui))
    
    githubText := settingsGui.Add("Text", "x100 y685 w60 h20 c4FC4FF BackgroundTrans", "GitHub")
    githubText.SetFont("s10 underline")
    githubText.OnEvent("Click", (*) => OpenUrl("https://github.com/fr0st-iwnl/Blip", settingsGui))
    
    ; Separator line
    settingsGui.Add("Text", "x20 y720 w360 h1 Background555555")
    
    ; === Action Buttons ===
    applyBtn := CreateTextButton(settingsGui, "Apply", 20, 740, 110, 35, (*) => ApplySettings(initialSizeEdit, maxSizeEdit, maxOpacityEdit, expandSpeedEdit, fadeStartPercentEdit, blockIBeamCheck, blockHandCheck, blockTaskbarCheck, blockGamesCheck, runOnStartupCheck, shapeDropDown))
    
    resetBtn := CreateTextButton(settingsGui, "Reset Defaults", 140, 740, 110, 35, (*) => ResetDefaults(initialSizeEdit, maxSizeEdit, colorPreview, colorPreviewBox, maxOpacityEdit, expandSpeedEdit, fadeStartPercentEdit, blockIBeamCheck, blockHandCheck, blockTaskbarCheck, blockGamesCheck, runOnStartupCheck, settingsGui, shapeDropDown))
    
    checkUpdatesBtn := CreateTextButton(settingsGui, "Check for Updates", 260, 740, 120, 35, (*) => CheckForUpdatesAsync(settingsGui, checkUpdatesBtn))
    
    settingsGui.OnEvent("Close", (*) => (settingsGui := ""))
    
    settingsGui.Show("w400 h795")
    WinActivate("Blip ahk_id " settingsGui.Hwnd)
}

GetShapeIndex(shape) {
    shapes := ["Circle", "Square", "Hexagon", "Diamond"]
    Loop shapes.Length {
        if (shapes[A_Index] = shape)
            return A_Index
    }
    return 1
}

UpdatePreviewOpacity(maxOpacityEdit) {
    global shapePreviewCtrl
    
    if !IsSet(shapePreviewCtrl) || !shapePreviewCtrl
        return
    
    try {
        opacity := IsNumber(maxOpacityEdit.Value) ? Integer(maxOpacityEdit.Value) : 255
        opacity := Min(255, Max(0, opacity))
        WinSetTransparent(opacity, shapePreviewCtrl)
    }
}

UpdateColorFromHex(colorPreview, colorPreviewBox, maxOpacityEdit, shapeDropDown) {
    global shadowColor, shapePreviewCtrl, shapePreviewCtrls, currentSettingsGui
    
    hexValue := Trim(colorPreview.Value)
    hexValue := RegExReplace(hexValue, "^#", "")
    
    if !RegExMatch(hexValue, "^[0-9A-Fa-f]{6}$")
        return
    
    shadowColor := StrUpper(hexValue)
    
    colorPreviewBox.Opt("Background" shadowColor)
    colorPreviewBox.Redraw()
    
    for ctrl in shapePreviewCtrls {
        try {
            WinSetRegion("", ctrl)
            ctrl.Visible := false
            ctrl.Destroy()
        }
    }
    shapePreviewCtrls := []
    
    Sleep(50)
    
    currentShape := shapeDropDown.Text
    opacity := IsNumber(maxOpacityEdit.Value) ? Integer(maxOpacityEdit.Value) : 255
    
    shapePreviewCtrl := currentSettingsGui.Add("Text", "x250 y65 w50 h50 Background" shadowColor)
    shapePreviewCtrls.Push(shapePreviewCtrl)
    
    SetShapeRegion(shapePreviewCtrl, 50, currentShape)
    WinSetTransparent(opacity, shapePreviewCtrl)
    shapePreviewCtrl.Redraw()
}

RecreateShapePreview(shapeDropDown, maxOpacityEdit) {
    global shapePreviewCtrl, shapePreviewCtrls, currentSettingsGui, shadowColor
    
    for ctrl in shapePreviewCtrls {
        try {
            WinSetRegion("", ctrl)
            ctrl.Visible := false
            ctrl.Destroy()
        }
    }
    shapePreviewCtrls := []
    
    Sleep(50)
    
    currentShape := shapeDropDown.Text
    opacity := IsNumber(maxOpacityEdit.Value) ? Integer(maxOpacityEdit.Value) : 255
    
    shapePreviewCtrl := currentSettingsGui.Add("Text", "x250 y65 w50 h50 Background" shadowColor)
    shapePreviewCtrls.Push(shapePreviewCtrl)
    
    SetShapeRegion(shapePreviewCtrl, 50, currentShape)
    WinSetTransparent(opacity, shapePreviewCtrl)
    
    shapePreviewCtrl.Redraw()
}

OpenUrl(url, parentGui) {
    try {
        Run(url)
    } catch Error as err {
        ShowMsgBox("Failed to open URL: " err.Message, "Error", "OK 0x10")
    }
}

; Choose Color
ChooseColor(colorPreview, colorPreviewBox, previewCtrl, maxOpacityEdit, shapeDropDown) {
    global shadowColor, configFile, shapePreviewCtrl, shapePreviewCtrls, currentSettingsGui
    
    chosenColor := ChooseColorDialog(shadowColor)
    if (chosenColor != "") {
        shadowColor := chosenColor
        colorPreview.Value := shadowColor
        colorPreviewBox.Opt("Background" shadowColor)
        colorPreviewBox.Redraw()
        
        for ctrl in shapePreviewCtrls {
            try {
                WinSetRegion("", ctrl)
                ctrl.Visible := false
                ctrl.Destroy()
            }
        }
        shapePreviewCtrls := []
        
        Sleep(50)
        
        currentShape := shapeDropDown.Text
        opacity := IsNumber(maxOpacityEdit.Value) ? Integer(maxOpacityEdit.Value) : 255
        
        shapePreviewCtrl := currentSettingsGui.Add("Text", "x250 y65 w50 h50 Background" shadowColor)
        shapePreviewCtrls.Push(shapePreviewCtrl)
        
        SetShapeRegion(shapePreviewCtrl, 50, currentShape)
        WinSetTransparent(opacity, shapePreviewCtrl)
        shapePreviewCtrl.Redraw()
        
        IniWrite(shadowColor, configFile, "Settings", "ShadowColor")
    }
}

; Apply Settings
ApplySettings(initialSizeEdit, maxSizeEdit, maxOpacityEdit, expandSpeedEdit, fadeStartPercentEdit, blockIBeamCheck, blockHandCheck, blockTaskbarCheck, blockGamesCheck, runOnStartupCheck, shapeDropDown) {
    global initialSize, maxSize, maxOpacity, expandSpeed, fadeStartPercent, configFile, blockIBeam, blockHand, blockTaskbar, blockGames, runOnStartup, shadowShape, settingsGui
    
    newInitialSize := IsNumber(initialSizeEdit.Value) ? Integer(initialSizeEdit.Value) : initialSize
    newMaxSize := IsNumber(maxSizeEdit.Value) ? Integer(maxSizeEdit.Value) : maxSize
    
    if (newInitialSize > 1000) {
        ShowMsgBox("Initial Size cannot exceed 1000 pixels.", "Invalid Size", "OK 0x30")
        initialSizeEdit.Value := initialSize
        return
    }
    
    if (newMaxSize > 1000) {
        ShowMsgBox("Max Size cannot exceed 1000 pixels.", "Invalid Size", "OK 0x30")
        maxSizeEdit.Value := maxSize
        return
    }
    
    initialSize := newInitialSize
    maxSize := newMaxSize
    maxOpacity := IsNumber(maxOpacityEdit.Value) ? Integer(maxOpacityEdit.Value) : maxOpacity
    expandSpeed := IsNumber(expandSpeedEdit.Value) ? Integer(expandSpeedEdit.Value) : expandSpeed
    
    fadeStartPercent := IsNumber(fadeStartPercentEdit.Value) ? Round(Float(fadeStartPercentEdit.Value), 2) : fadeStartPercent
    
    blockIBeam := blockIBeamCheck.Value
    blockHand := blockHandCheck.Value
    blockTaskbar := blockTaskbarCheck.Value
    blockGames := blockGamesCheck.Value
    runOnStartup := runOnStartupCheck.Value
    shadowShape := shapeDropDown.Text
    
    SetStartup(runOnStartup)
    
    IniWrite(initialSize, configFile, "Settings", "InitialSize")
    IniWrite(maxSize, configFile, "Settings", "MaxSize")
    IniWrite(maxOpacity, configFile, "Settings", "MaxOpacity")
    IniWrite(expandSpeed, configFile, "Settings", "ExpandSpeed")
    IniWrite(fadeStartPercent, configFile, "Settings", "FadeStartPercent")
    IniWrite(blockIBeam, configFile, "Settings", "BlockIBeam")
    IniWrite(blockHand, configFile, "Settings", "BlockHand")
    IniWrite(blockTaskbar, configFile, "Settings", "BlockTaskbar")
    IniWrite(blockGames, configFile, "Settings", "BlockGames")
    IniWrite(runOnStartup, configFile, "Settings", "RunOnStartup")
    IniWrite(shadowShape, configFile, "Settings", "ShadowShape")
}

SetStartup(enable) {
    startupKey := "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
    appName := "Blip"
    
    if (enable) {
        RegWrite(A_ScriptFullPath, "REG_SZ", startupKey, appName)
    } else {
        try RegDelete(startupKey, appName)
    }
}

; Reset settings to Default
ResetDefaults(initialSizeEdit, maxSizeEdit, colorPreview, colorPreviewBox, maxOpacityEdit, expandSpeedEdit, fadeStartPercentEdit, blockIBeamCheck, blockHandCheck, blockTaskbarCheck, blockGamesCheck, runOnStartupCheck, settingsGui, shapeDropDown) {
    global initialSize, maxSize, shadowColor, maxOpacity, expandSpeed, fadeStartPercent, configFile, blockIBeam, blockHand, blockTaskbar, blockGames, runOnStartup, shadowShape, shapePreviewCtrl, shapePreviewCtrls, currentSettingsGui
    
    result := ShowMsgBox("Are you sure you want to reset all settings to defaults?", "Confirm Reset", "YesNo 0x4")
    if (result != "Yes")
        return
    
    initialSize := 15
    maxSize := 40
    shadowColor := "000000"
    maxOpacity := 60
    expandSpeed := 6
    fadeStartPercent := 0.5
    blockIBeam := true
    blockHand := false
    blockTaskbar := true
    blockGames := true
    runOnStartup := true
    shadowShape := "Circle"
    
    initialSizeEdit.Value := initialSize
    maxSizeEdit.Value := maxSize
    colorPreview.Value := shadowColor
    maxOpacityEdit.Value := maxOpacity
    expandSpeedEdit.Value := expandSpeed
    fadeStartPercentEdit.Value := Format("{:.2f}", fadeStartPercent)
    blockIBeamCheck.Value := blockIBeam
    blockHandCheck.Value := blockHand
    blockTaskbarCheck.Value := blockTaskbar
    blockGamesCheck.Value := blockGames
    runOnStartupCheck.Value := runOnStartup
    shapeDropDown.Choose(1)
    
    colorPreviewBox.Opt("Background" shadowColor)
    colorPreviewBox.Redraw()
    
    for ctrl in shapePreviewCtrls {
        try {
            WinSetRegion("", ctrl)
            ctrl.Visible := false
            ctrl.Destroy()
        }
    }
    shapePreviewCtrls := []
    
    Sleep(50)
    
    shapePreviewCtrl := currentSettingsGui.Add("Text", "x250 y65 w50 h50 Background" shadowColor)
    shapePreviewCtrls.Push(shapePreviewCtrl)
    SetShapeRegion(shapePreviewCtrl, 50, shadowShape)
    WinSetTransparent(maxOpacity, shapePreviewCtrl)
    shapePreviewCtrl.Redraw()
    
    SetStartup(runOnStartup)
    
    IniWrite(initialSize, configFile, "Settings", "InitialSize")
    IniWrite(maxSize, configFile, "Settings", "MaxSize")
    IniWrite(shadowColor, configFile, "Settings", "ShadowColor")
    IniWrite(maxOpacity, configFile, "Settings", "MaxOpacity")
    IniWrite(expandSpeed, configFile, "Settings", "ExpandSpeed")
    IniWrite(fadeStartPercent, configFile, "Settings", "FadeStartPercent")
    IniWrite(blockIBeam, configFile, "Settings", "BlockIBeam")
    IniWrite(blockHand, configFile, "Settings", "BlockHand")
    IniWrite(blockTaskbar, configFile, "Settings", "BlockTaskbar")
    IniWrite(blockGames, configFile, "Settings", "BlockGames")
    IniWrite(runOnStartup, configFile, "Settings", "RunOnStartup")
    IniWrite(shadowShape, configFile, "Settings", "ShadowShape")
}

CheckForUpdatesAsync(parentGui, checkUpdatesBtn) {
    global isCheckingForUpdates
    
    if (isCheckingForUpdates || WinExist("Update Available") || WinExist("Update Check Failed") || WinExist("No Updates Available"))
        return
    
    isCheckingForUpdates := true
    checkUpdatesBtn.Text := "Checking..."
    
    SetTimer(() => CheckForUpdates(true, parentGui, checkUpdatesBtn), -10)
}

; CheckForUpdates Function
CheckForUpdates(showMessages := false, parentGui?, checkUpdatesBtn?) {
    global currentVersion, versionCheckUrl, isCheckingForUpdates
    
    if (!InternetCheckConnection()) {
        if (showMessages) {
            ShowMsgBox("Unable to check for updates. Please check your internet connection.", "Update Check Failed", "OK 0x30")
        }
        if (IsSet(checkUpdatesBtn)) {
            checkUpdatesBtn.Enabled := true
            checkUpdatesBtn.Text := "Check for Updates"
        }
        isCheckingForUpdates := false
        return
    }
    
    try {
        whr := ComObject("WinHttp.WinHttpRequest.5.1")
        whr.Open("GET", versionCheckUrl, true)
        whr.SetTimeouts(5000, 5000, 5000, 5000)
        whr.Send()
        whr.WaitForResponse(5)
        
        if (whr.Status = 200) {
            latestVersion := RegExReplace(Trim(whr.ResponseText), "[\r\n\s]")
            currentVersion := Trim(currentVersion)
            
            if (latestVersion != currentVersion) {
                result := ShowMsgBox(
                    "A new version of Blip is available!`n`n"
                    . "Current version: " currentVersion "`n"
                    . "Latest version: " latestVersion "`n`n"
                    . "Visit the download page?",
                    "Update Available",
                    "YesNo 0x40"
                )
                if (result = "Yes")
                    Run("https://github.com/fr0st-iwnl/Blip/releases")
            } else if (showMessages) {
                ShowMsgBox("You have the latest version: " currentVersion, "No Updates Available", "OK 0x40")
            }
        } else if (showMessages) {
            ShowMsgBox("Failed to check for updates (Status: " whr.Status ")", "Update Check Failed", "OK 0x30")
        }
    } catch Error as err {
        if (showMessages) {
            ShowMsgBox("Failed to check for updates: " err.Message, "Update Check Failed", "OK 0x30")
        }
    }
    
    if (IsSet(checkUpdatesBtn)) {
        checkUpdatesBtn.Enabled := true
        checkUpdatesBtn.Text := "Check for Updates"
    }
    isCheckingForUpdates := false
}

InternetCheckConnection(url := "https://www.google.com") {
    try {
        http := ComObject("WinHttp.WinHttpRequest.5.1")
        http.Open("HEAD", url, true)
        http.SetTimeouts(2000, 2000, 2000, 2000)
        http.Send()
        http.WaitForResponse(2)
        return http.Status = 200
    } catch {
        return false
    }
}

ChooseColorDialog(currentColor) {
    if (StrLen(currentColor) = 6) {
        r := Format("{:d}", "0x" . SubStr(currentColor, 1, 2))
        g := Format("{:d}", "0x" . SubStr(currentColor, 3, 2))
        b := Format("{:d}", "0x" . SubStr(currentColor, 5, 2))
        bgrColor := (b << 16) | (g << 8) | r
    } else {
        bgrColor := 0
    }
    
    customColors := Buffer(64, 0)
    cc := Buffer(9 * A_PtrSize, 0)
    NumPut("UInt", 9 * A_PtrSize, cc, 0)
    NumPut("Ptr", A_ScriptHwnd, cc, A_PtrSize)
    NumPut("UInt", bgrColor, cc, 3 * A_PtrSize)
    NumPut("Ptr", customColors.Ptr, cc, 4 * A_PtrSize)
    NumPut("UInt", 0x00000100 | 0x00000002, cc, 5 * A_PtrSize)
    
    if (DllCall("comdlg32\ChooseColor", "Ptr", cc)) {
        bgrResult := NumGet(cc, 3 * A_PtrSize, "UInt")
        r := Format("{:02X}", (bgrResult & 0xFF))
        g := Format("{:02X}", (bgrResult >> 8) & 0xFF)
        b := Format("{:02X}", (bgrResult >> 16) & 0xFF)
        return r . g . b
    }
    return ""
}

ExitScript(*) {
    ExitApp
}

; ╔════════════════════════════════════════════════════════════════════════════════════════════════════════╗
; ║                                          LIBRARIES                                                     ║
; ╚════════════════════════════════════════════════════════════════════════════════════════════════════════╝

;==================================================
;              TEXT BUTTON LIBRARY
;==================================================

/************************************************************************
* @description Text-based button library for AutoHotkey v2 with hover
*              and click effects, customizable colors, and safe timers.
* @file TextButton.ahk
* @link https://github.com/QuintixLabs/TextButton.ahk
* @author QuintixLabs / fr0st
* @date 10/18/2025
* @version 1.0
***********************************************************************/

CreateTextButton(parentGui, text, x, y, w, h, callback) {
    border := parentGui.Add("Text", "x" x " y" y " w" w " h" h " Background555555")
    btn := parentGui.Add("Text", "x" (x+1) " y" (y+1) " w" (w-2) " h" (h-2) " Background363636 Center 0x200", text)
    btn.SetFont("s10 cFFFFFF", "Segoe UI")
    
    btn._callback := callback
    btn._border := border
    btn._origText := text
    btn._isHovered := false
    
    btn.OnEvent("Click", (*) => HandleButtonClick(btn))
    
    DllCall("SetWindowSubclass", "Ptr", btn.Hwnd, "Ptr", CallbackCreate(ButtonWndProc), "Ptr", btn.Hwnd, "Ptr", ObjPtr(btn))
    
    return btn
}

HandleButtonClick(btn) {
    if (btn.HasProp("_clicking") && btn._clicking)
        return
    
    btn._clicking := true
    
    btn.Opt("Background666666")
    btn.Redraw()
    
    btn._callback.Call()
    
    SetTimer(() => (
        btn._clicking := false,
        btn._isHovered ? (
            btn.Opt("Background4a4a4a")
        ) : (
            btn.Opt("Background363636")
        ),
        btn.Redraw()
    ), -100)
}

ButtonWndProc(hWnd, uMsg, wParam, lParam, uIdSubclass, dwRefData) {
    static WM_MOUSEMOVE := 0x200
    static WM_MOUSELEAVE := 0x2A3
    static TME_LEAVE := 0x00000002
    
    btn := ObjFromPtrAddRef(dwRefData)
    
    switch uMsg {
        case WM_MOUSEMOVE:
            if (!btn._isHovered) {
                btn._isHovered := true
                btn.Opt("Background4a4a4a")
                btn.Redraw()
                
                tme := Buffer(A_PtrSize = 8 ? 24 : 16, 0)
                NumPut("UInt", tme.Size, tme, 0)
                NumPut("UInt", TME_LEAVE, tme, 4)
                NumPut("Ptr", hWnd, tme, 8)
                NumPut("UInt", 0, tme, 8 + A_PtrSize)
                
                DllCall("TrackMouseEvent", "Ptr", tme)
            }
            
        case WM_MOUSELEAVE:
            if (btn._isHovered) {
                btn._isHovered := false
                btn.Opt("Background363636")
                btn._border.Opt("Background555555")
                btn.Redraw()
                btn._border.Redraw()
            }
    }
    
    return DllCall("DefSubclassProc", "Ptr", hWnd, "UInt", uMsg, "Ptr", wParam, "Ptr", lParam, "Ptr")
}

;==================================================
;              DARK MSGBOX LIBRARY
;==================================================

/************************************************************************
* @description Apply dark theme to the built-in MsgBox and InputBox.
* @file Dark_MsgBox.ahk
* @link https://github.com/nperovic/DarkMsgBox
* @author Nikola Perovic
* @date 2024/06/16
* @version 1.1.0
***********************************************************************/

#DllLoad gdi32.dll

class DarkMsgBox
{
    static __hbrush := []
    static __hbrushInit := false

    static CreateSolidBrush(crColor) => DllCall('Gdi32\CreateSolidBrush', 'uint', crColor, 'ptr')
    static DeleteObject(hObject) => DllCall('Gdi32\DeleteObject', 'ptr', hObject, 'int')
    static DestroyIcon(hIcon) => DllCall('DestroyIcon', 'ptr', hIcon)
    static CallWindowProc(lpPrevWndFunc, hWnd, uMsg, wParam, lParam) =>
        DllCall("CallWindowProc", "Ptr", lpPrevWndFunc, "Ptr", hWnd, "UInt", uMsg, "Ptr", wParam, "Ptr", lParam)

    static __New()
    {
        static _Msgbox := MsgBox.Call.Bind(MsgBox)
        static _InputBox := InputBox.Call.Bind(InputBox)
        MsgBox.DefineProp("Call", {Call: CallNativeFunc})
        InputBox.DefineProp("Call", {Call: CallNativeFunc})

        CallNativeFunc(_this, params*)
        {
            static WM_COMMNOTIFY := 0x44
            static WM_INITDIALOG := 0x0110
            
            iconNumber := 1
            iconFile := ""
            
            if (params.Length = (_this.MaxParams + 2))
                iconNumber := params.Pop()
            
            if (params.Length = (_this.MaxParams + 1)) 
                iconFile := params.Pop()
            
            SetThreadDpiAwarenessContext(-3)
    
            if InStr(_this.Name, "MsgBox")
                OnMessage(WM_COMMNOTIFY, ON_WM_COMMNOTIFY)
            else
                OnMessage(WM_INITDIALOG, ON_WM_INITDIALOG, -1)
    
            return _%_this.name%(params*)
    
            ON_WM_INITDIALOG(wParam, lParam, msg, hwnd)
            {
                OnMessage(WM_INITDIALOG, ON_WM_INITDIALOG, 0)
                WNDENUMPROC(hwnd)
            }
            
            ON_WM_COMMNOTIFY(wParam, lParam, msg, hwnd)
            {
                if (msg = 68 && wParam = 1027) {
                    OnMessage(0x44, ON_WM_COMMNOTIFY, 0)
                    DllCall("User32\EnumThreadWindows", "UInt", DllCall("Kernel32\GetCurrentThreadId", "UInt"), "Ptr", CallbackCreate(WNDENUMPROC), "Ptr", 0)
                }
            }

            WNDENUMPROC(hwnd, *)
            {
                static SM_CICON := "W" SysGet(11) " H" SysGet(12)
                static SM_CSMICON := "W" SysGet(49) " H" SysGet(50)
                static ICON_BIG := 1
                static ICON_SMALL := 0
                static WM_SETICON := 0x80
                static WS_CLIPCHILDREN := 0x02000000
                static WS_CLIPSIBLINGS := 0x04000000
                static WS_EX_COMPOSITED := 0x02000000
                static winAttrMap := Map(10, true, 17, true, 20, true, 38, 4, 35, 0x2b2b2b)
    
                SetWinDelay(-1)
                SetControlDelay(-1)
                DetectHiddenWindows(true)
    
                if !WinExist("ahk_class #32770 ahk_id" hwnd)
                    return 1
    
                WinSetStyle("+" (WS_CLIPSIBLINGS | WS_CLIPCHILDREN))
                WinSetExStyle("+" (WS_EX_COMPOSITED))
                SetWindowTheme(hwnd, "DarkMode_Explorer")
    
                if iconFile {
                    hICON_SMALL := LoadPicture(iconFile, SM_CSMICON " Icon" iconNumber, &handleType)
                    hICON_BIG := LoadPicture(iconFile, SM_CICON " Icon" iconNumber, &handleType)
                    PostMessage(WM_SETICON, ICON_SMALL, hICON_SMALL)
                    PostMessage(WM_SETICON, ICON_BIG, hICON_BIG)
                }
    
                for dwAttribute, pvAttribute in winAttrMap
                    DwmSetWindowAttribute(hwnd, dwAttribute, pvAttribute)
                
                GWL_WNDPROC(hwnd, hICON_SMALL?, hICON_BIG?)
                return 0
            }
            
            GWL_WNDPROC(winId := "", hIcons*)
            {
                static SetWindowLong := DllCall.Bind(A_PtrSize = 8 ? "SetWindowLongPtr" : "SetWindowLong", "ptr",, "int",, "ptr",, "ptr")
                static BS_FLAT := 0x8000
                static BS_BITMAP := 0x0080
                static DPI := (A_ScreenDPI / 96)
                static WM_CLOSE := 0x0010
                static WM_CTLCOLORBTN := 0x0135
                static WM_CTLCOLORDLG := 0x0136
                static WM_CTLCOLOREDIT := 0x0133
                static WM_CTLCOLORSTATIC := 0x0138
                static WM_DESTROY := 0x0002
                static WM_SETREDRAW := 0x000B
    
                SetControlDelay(-1)
    
                btns := []
                btnHwnd := ""
    
                for ctrl in WinGetControlsHwnd(winId)
                {
                    classNN := ControlGetClassNN(ctrl)
                    SetWindowTheme(ctrl, !InStr(classNN, "Edit") ? "DarkMode_Explorer" : "DarkMode_CFD")
                    if InStr(classNN, "B") 
                        btns.Push(btnHwnd := ctrl)
                }
    
                WindowProcOld := SetWindowLong(winId, -4, CallbackCreate(WNDPROC))
                
                WNDPROC(hwnd, uMsg, wParam, lParam)
                {
                    SetWinDelay(-1)
                    SetControlDelay(-1)

                    if (!DarkMsgBox.__hbrushInit) {
                        for clr in [0x202020, 0x2b2b2b]
                            DarkMsgBox.__hbrush.Push(DarkMsgBox.CreateSolidBrush(clr))
                        DarkMsgBox.__hbrushInit := true
                    }

                    switch uMsg {
                    case WM_CTLCOLORSTATIC: 
                    {
                        SelectObject(wParam, DarkMsgBox.__hbrush[2])
                        SetBkMode(wParam, 0)
                        SetTextColor(wParam, 0xFFFFFF)
                        SetBkColor(wParam, 0x2b2b2b)

                        for _hwnd in btns
                            PostMessage(WM_SETREDRAW,,,_hwnd)

                        GetClientRect(winId, rcC := this.RECT())
                        WinGetClientPos(&winX, &winY, &winW, &winH, winId)
                        ControlGetPos(, &btnY,, &btnH, btnHwnd)
                        hdc := GetDC(winId)
                        rcC.top := btnY - (rcC.bottom - (btnY+btnH))
                        rcC.bottom *= 2
                        rcC.right *= 2
                        
                        SetBkMode(hdc, 0)
                        FillRect(hdc, rcC, DarkMsgBox.__hbrush[1])
                        ReleaseDC(winId, hdc)

                        for _hwnd in btns
                            PostMessage(WM_SETREDRAW, 1,,_hwnd)

                        return DarkMsgBox.__hbrush[2]
                    }
                    case WM_CTLCOLORBTN, WM_CTLCOLORDLG, WM_CTLCOLOREDIT: 
                    {
                        brushIndex := !(uMsg = WM_CTLCOLORBTN)
                        SelectObject(wParam, brush := DarkMsgBox.__hbrush[brushIndex+1])
                        SetBkMode(wParam, 0)
                        SetTextColor(wParam, 0xFFFFFF)
                        SetBkColor(wParam, !brushIndex ? 0x202020 : 0x2b2b2b)
                        return brush
                    }
                    case WM_DESTROY: 
                    {
                        for v in hIcons
                            (v??0) && DarkMsgBox.DestroyIcon(v)

                        while DarkMsgBox.__hbrush.Length
                            DarkMsgBox.DeleteObject(DarkMsgBox.__hbrush.Pop())

                        DarkMsgBox.__hbrushInit := false
                    }}
                    return DarkMsgBox.CallWindowProc(WindowProcOld, hwnd, uMsg, wParam, lParam)
                }
            }
    
            DwmSetWindowAttribute(hwnd, dwAttribute, pvAttribute, cbAttribute := 4) =>
                DllCall("Dwmapi\DwmSetWindowAttribute", "Ptr", hwnd, "UInt", dwAttribute, "Ptr*", &pvAttribute, "UInt", cbAttribute)
            
            FillRect(hDC, lprc, hbr) => DllCall("User32\FillRect", "ptr", hDC, "ptr", lprc, "ptr", hbr, "int")
            GetClientRect(hWnd, lpRect) => DllCall("User32\GetClientRect", "ptr", hWnd, "ptr", lpRect, "int")
            GetCurrentThreadId() => DllCall("Kernel32\GetCurrentThreadId", "uint")
            GetDC(hwnd := 0) => DllCall("GetDC", "ptr", hwnd, "ptr")
            ReleaseDC(hWnd, hDC) => DllCall("User32\ReleaseDC", "ptr", hWnd, "ptr", hDC, "int")
            SelectObject(hdc, hgdiobj) => DllCall('Gdi32\SelectObject', 'ptr', hdc, 'ptr', hgdiobj, 'ptr')
            SetBkColor(hdc, crColor) => DllCall('Gdi32\SetBkColor', 'ptr', hdc, 'uint', crColor, 'uint')
            SetBkMode(hdc, iBkMode) => DllCall('Gdi32\SetBkMode', 'ptr', hdc, 'int', iBkMode, 'int')
            SetTextColor(hdc, crColor) => DllCall('Gdi32\SetTextColor', 'ptr', hdc, 'uint', crColor, 'uint')
            SetThreadDpiAwarenessContext(dpiContext) => DllCall("SetThreadDpiAwarenessContext", "ptr", dpiContext, "ptr")
            SetWindowTheme(hwnd, pszSubAppName, pszSubIdList := "") =>
                (!DllCall("uxtheme\SetWindowTheme", "ptr", hwnd, "ptr", StrPtr(pszSubAppName), "ptr", pszSubIdList ? StrPtr(pszSubIdList) : 0) ? true : false)
        }
    }

    class RECT extends Buffer {
        static ofst := Map("left", 0, "top", 4, "right", 8, "bottom", 12)

        __New(left := 0, top := 0, right := 0, bottom := 0) {
            super.__New(16)
            NumPut("int", left, "int", top, "int", right, "int", bottom, this)
        }

        __Set(Key, Params, Value) {
            if DarkMsgBox.RECT.ofst.Has(k := StrLower(key))
                NumPut("int", value, this, DarkMsgBox.RECT.ofst[k])
            else throw PropertyError
        }

        __Get(Key, Params) {
            if DarkMsgBox.RECT.ofst.Has(k := StrLower(key))
                return NumGet(this, DarkMsgBox.RECT.ofst[k], "int")
            throw PropertyError
        }

        width => this.right - this.left
        height => this.bottom - this.top
    }
}