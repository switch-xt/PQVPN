const fs = require('fs');
let c = fs.readFileSync('src/components/Icons.tsx', 'utf8');
c = c.replace(/export const (\w+): React\.FC<\{ className\?: string \}> = \(\{ className \}\) => \(\r?\n  <svg className=\{className\}/g, 'export const $1: React.FC<React.SVGProps<SVGSVGElement>> = (props) => (\n  <svg {...props}');
fs.writeFileSync('src/components/Icons.tsx', c);
