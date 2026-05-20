Add-Type -AssemblyName 'System.IO.Compression.FileSystem'
$path = 'C:\Users\AYUSH KUMAR\Desktop\vpnX\PQVPN.docx'
$destDir = 'C:\Users\AYUSH KUMAR\Desktop\vpnX\docx_extracted'
if (Test-Path $destDir) { Remove-Item $destDir -Recurse -Force }
[System.IO.Compression.ZipFile]::ExtractToDirectory($path, $destDir)
Get-ChildItem -Recurse $destDir | Select-Object FullName, Length
