const fs = require('fs');
const dir = 'D:/3_Code_Projects/DeepFlow/src-tauri/icons';
const files = fs.readdirSync(dir);
for (const f of files) {
  const stat = fs.statSync(dir + '/' + f);
  console.log(f, 'dir=' + stat.isDirectory(), 'size=' + stat.size);
}
