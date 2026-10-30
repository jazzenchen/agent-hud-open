$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$result = @(
    Get-CimInstance Win32_Process -Filter "Name='language_server.exe' OR Name='agy.exe'" -Property ProcessId,Name,CreationDate |
        ForEach-Object {
            $owner = Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid -ErrorAction SilentlyContinue
            if ($owner.ReturnValue -eq 0 -and $owner.Sid -eq $sid) {
                $details = Get-CimInstance Win32_Process -Filter "ProcessId=$($_.ProcessId)" -Property ProcessId,Name,CreationDate,ExecutablePath,CommandLine -ErrorAction SilentlyContinue
                if ($details -and $details.CreationDate -eq $_.CreationDate -and $details.ExecutablePath -and $details.CommandLine) {
                    @{ pid = [uint32]$details.ProcessId; executable = $details.ExecutablePath; command = $details.CommandLine }
                }
            }
        }
)
ConvertTo-Json -InputObject $result -Compress -Depth 3
