# UI Automation helpers for scripting the official SP-404MKII app during capture sessions.
# Usage: . .\tools\uia.ps1

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class SpWin {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
}
"@
[SpWin]::SetProcessDPIAware() | Out-Null

$SpExe = 'C:\Program Files (x86)\Roland\SP-404MKII\SP-404MKII.exe'
$script:SpAutomationElement = [System.Windows.Automation.AutomationElement]

function Get-SpWindow {
    $p = Get-Process SP-404MKII -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
    if (-not $p) { return $null }
    $script:SpAutomationElement::FromHandle($p.MainWindowHandle)
}

# Flat list of descendants: Type, Name, X, Y, W, H, El
function Get-SpElements {
    $win = Get-SpWindow
    if (-not $win) { throw 'SP-404MKII window not found' }
    $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) {
        $c = $e.Current; $r = $c.BoundingRectangle
        [pscustomobject]@{
            Type = $c.ControlType.ProgrammaticName -replace 'ControlType\.', ''
            Name = $c.Name; X = [int]$r.X; Y = [int]$r.Y; W = [int]$r.Width; H = [int]$r.Height; El = $e
        }
    }
}

function Find-SpElement([string]$Name, [string]$Type, [int]$Index = 0) {
    $m = @(Get-SpElements | Where-Object { (-not $Name -or $_.Name -eq $Name) -and (-not $Type -or $_.Type -eq $Type) })
    if ($m.Count -le $Index) { throw "element not found: '$Name' [$Type] #$Index" }
    $m[$Index]
}

function Invoke-SpElement($Item) {
    $Item.El.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Invoke-SpButton([string]$Name, [int]$Index = 0) {
    Invoke-SpElement (Find-SpElement -Name $Name -Type Button -Index $Index)
}

function Set-SpRange($Item, [double]$Value) {
    $Item.El.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue($Value)
}

function Get-SpRange($Item) {
    $v = $Item.El.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).Current
    [pscustomobject]@{ Value = $v.Value; Min = $v.Minimum; Max = $v.Maximum; Step = $v.SmallChange }
}

function Switch-SpToggle($Item) {
    $Item.El.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
}

# Click at screen coordinates by posting mouse messages to the app window
# (for custom-drawn JUCE components with no UIA patterns; does not move the real cursor).
function Send-SpClick([int]$X, [int]$Y) {
    $h = (Get-Process SP-404MKII | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1).MainWindowHandle
    $p = New-Object SpWin+POINT; $p.X = $X; $p.Y = $Y
    [SpWin]::ScreenToClient($h, [ref]$p) | Out-Null
    $l = [IntPtr](($p.Y -shl 16) -bor ($p.X -band 0xFFFF))
    [SpWin]::PostMessage($h, 0x0200, [IntPtr]0, $l) | Out-Null   # WM_MOUSEMOVE
    Start-Sleep -Milliseconds 50
    [SpWin]::PostMessage($h, 0x0201, [IntPtr]1, $l) | Out-Null   # WM_LBUTTONDOWN
    Start-Sleep -Milliseconds 80
    [SpWin]::PostMessage($h, 0x0202, [IntPtr]0, $l) | Out-Null   # WM_LBUTTONUP
}

# Pad cell centre in screen coordinates. Bank 0..9 (A..J), pad 1..16, from the Samples tab layout.
function Get-SpPadPoint([int]$Bank, [int]$Pad) {
    $cells = @(Get-SpElements | Where-Object { $_.Type -eq 'Custom' -and $_.W -eq 30 -and $_.H -eq 30 } |
        Sort-Object Y, X)
    if ($cells.Count -ne 160) { throw "expected 160 pad cells, found $($cells.Count) (is the Samples tab open?)" }
    # Rows of banks: A-E on top, F-J below; each bank is a 4x4 block.
    $blockX = @($cells | ForEach-Object X | Sort-Object -Unique)
    $blockY = @($cells | ForEach-Object Y | Sort-Object -Unique)
    $col = ($Bank % 5) * 4 + (($Pad - 1) % 4)
    $row = [math]::Floor($Bank / 5) * 4 + [math]::Floor(($Pad - 1) / 4)
    [pscustomobject]@{ X = $blockX[$col] + 15; Y = $blockY[$row] + 15 }
}

function Select-SpPad([int]$Bank, [int]$Pad) {
    $pt = Get-SpPadPoint $Bank $Pad
    Send-SpClick $pt.X $pt.Y
}

function Save-SpScreenshot([string]$Path) {
    $p = Get-Process SP-404MKII | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
    $r = New-Object SpWin+RECT
    [SpWin]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
    [SpWin]::PrintWindow($p.MainWindowHandle, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc); $g.Dispose()
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    $Path
}

function Invoke-Capd([string]$Command) {
    py -3 (Join-Path $PSScriptRoot 'capd.py') @($Command -split ' ')
}
