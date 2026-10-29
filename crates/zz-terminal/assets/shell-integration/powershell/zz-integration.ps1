if (Test-Path -LiteralPath 'Variable:__ZZ_TITLE_INTEGRATION_INSTALLED') { return }
$global:__ZZ_TITLE_INTEGRATION_INSTALLED = 1
$global:__ZZ_PREEXEC_INSTALLED = 0
$global:__ZZ_COMMAND_RUNNING = $false

$global:__ZZ_ESC = [char]27
$global:__ZZ_BEL = [char]7
$global:__ZZ_SHELL_NAME = 'powershell'
try { $global:__ZZ_SHELL_NAME = (Get-Process -Id $PID).ProcessName } catch { }

function global:__zz_write_title {
    param([string] $Value = '')

    $Value = $Value -replace "[`r`n]", ' '
    $Value = $Value -replace '\p{Cc}', ''
    if ($Value.Length -gt 512) { $Value = $Value.Substring(0, 512) }
    [Console]::Write("$($global:__ZZ_ESC)]2626$($global:__ZZ_BEL)$($global:__ZZ_ESC)]2;$Value$($global:__ZZ_BEL)")
}

function global:__zz_write_working_directory {
    if ($PWD.Provider.Name -ne 'FileSystem') { return }

    $value = $PWD.ProviderPath
    $value = $value -replace "[`r`n]", ''
    $value = $value -replace '\p{Cc}', ''
    if ($value.Length -gt 4096) { $value = $value.Substring(0, 4096) }
    $value = $value -replace '\\', '/'
    if (-not $value.StartsWith('/')) { $value = '/' + $value }
    $machine = [System.Environment]::MachineName
    [Console]::Write("$($global:__ZZ_ESC)]7;file://$machine$value$($global:__ZZ_BEL)")
}

function global:__zz_title_precmd {
    __zz_write_working_directory
    __zz_write_title $global:__ZZ_SHELL_NAME
    [Console]::Write("$($global:__ZZ_ESC)[0 q")
}

function global:__zz_title_preexec {
    param([string] $Command = '')

    if ([string]::IsNullOrWhiteSpace($Command)) { return }
    [Console]::Write("$($global:__ZZ_ESC)[0 q")
    __zz_write_title $Command
    $global:__ZZ_COMMAND_RUNNING = $true
    [Console]::Write("$($global:__ZZ_ESC)]133;C$($global:__ZZ_BEL)")
}

function global:__zz_install_preexec_hook {
    if ($global:__ZZ_PREEXEC_INSTALLED) { return }
    if (-not (Test-Path -LiteralPath 'Function:PSConsoleHostReadLine')) { return }

    $global:__ZZ_PREEXEC_INSTALLED = 1
    $global:__ZZ_READ_LINE_ORIGINAL = $function:PSConsoleHostReadLine
    function global:PSConsoleHostReadLine {
        $__zz_line = & $global:__ZZ_READ_LINE_ORIGINAL
        try { __zz_title_preexec $__zz_line } catch { }
        $__zz_line
    }
}

if (Test-Path -LiteralPath 'Function:prompt') {
    $global:__ZZ_PROMPT_ORIGINAL = $function:prompt
} else {
    $global:__ZZ_PROMPT_ORIGINAL = {
        "PS $($ExecutionContext.SessionState.Path.CurrentLocation)$('>' * ($NestedPromptLevel + 1)) "
    }
}

function global:prompt {
    $__zz_success = $global:?
    $__zz_native_exit = $global:LASTEXITCODE
    if ($global:__ZZ_COMMAND_RUNNING) {
        $__zz_exit = 0
        if (-not $__zz_success) {
            $__zz_exit = 1
            if ($null -ne $__zz_native_exit -and $__zz_native_exit -ne 0) {
                $__zz_exit = $__zz_native_exit
            }
        }
        [Console]::Write("$($global:__ZZ_ESC)]133;D;$__zz_exit$($global:__ZZ_BEL)")
        $global:__ZZ_COMMAND_RUNNING = $false
    }
    [Console]::Write("$($global:__ZZ_ESC)]133;A$($global:__ZZ_BEL)")
    if (-not $__zz_success) { Write-Error 'command failed' -ErrorAction Ignore }
    $__zz_rendered = & $global:__ZZ_PROMPT_ORIGINAL
    try {
        __zz_install_preexec_hook
        __zz_title_precmd
    } catch { }
    "$__zz_rendered$($global:__ZZ_ESC)]133;B$($global:__ZZ_BEL)"
}

Remove-Item -LiteralPath 'Env:ZZ_SHELL_INTEGRATION_ACTIVE' -ErrorAction SilentlyContinue
