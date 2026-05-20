$raw = Get-Content 'C:\Users\AYUSH KUMAR\Desktop\vpnX\docx_extracted\word\document.xml' -Raw
$matches = [regex]::Matches($raw, '<w:t[^>]*>([^<]+)</w:t>')
$sb = New-Object System.Text.StringBuilder
foreach ($m in $matches) {
    [void]$sb.Append($m.Groups[1].Value)
}
$sb.ToString().Substring(0, [Math]::Min(10000, $sb.Length)) | Out-File 'C:\Users\AYUSH KUMAR\Desktop\vpnX\docx_text.txt' -Encoding UTF8
Write-Output "Extracted $($sb.Length) chars total, wrote first 10000"
