# Log state changes of the official app's UI with epoch timestamps, to correlate manual
# actions with capture segments. Stops when the stop file exists.
#   powershell -File tools\uiwatch.ps1 -Out <log> -Stop <stopfile>
param([string]$Out, [string]$Stop)

. (Join-Path $PSScriptRoot 'uia.ps1')
$prev = @{}
while (-not (Test-Path $Stop)) {
    try {
        $state = @{}
        foreach ($e in Get-SpElements) {
            if ($e.Type -in 'ComboBox', 'CheckBox', 'Text' -and $e.X -gt 0) {
                $val = $e.Name
                if ($e.Type -eq 'CheckBox') {
                    try { $val += '=' + $e.El.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState } catch {}
                }
                $state["$($e.Type)@$($e.X),$($e.Y)"] = $val
            }
        }
        $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
        foreach ($k in $state.Keys) {
            if ($prev.ContainsKey($k) -and $prev[$k] -ne $state[$k]) {
                Add-Content -Path $Out -Value ("{0:F3}`t{1}`t{2} -> {3}" -f $now, $k, $prev[$k], $state[$k])
            }
        }
        $prev = $state
    } catch {}
    Start-Sleep -Milliseconds 250
}
