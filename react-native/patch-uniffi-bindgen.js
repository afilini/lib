#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// File paths relative to script location
const basePath = './node_modules/uniffi-bindgen-react-native/typescript/src';
const ffiConvertersPath = path.join(basePath, 'ffi-converters.ts');
const ffiTypesPath = path.join(basePath, 'ffi-types.ts');

function patchFile(filePath, patches, fileName) {
  try {
    // Check if file exists
    if (!fs.existsSync(filePath)) {
      console.error(`❌ File not found: ${filePath}`);
      return false;
    }

    // Read the file
    let content = fs.readFileSync(filePath, 'utf8');
    
    console.log(`📝 Patching ${fileName}...`);
    
    // Apply patches
    let patchCount = 0;
    patches.forEach((patch, index) => {
      const originalContent = content;
      content = content.replace(patch.from, patch.to);
      if (content !== originalContent) {
        patchCount++;
        console.log(`  ✅ Applied patch ${index + 1}: ${patch.description}`);
      } else {
        console.log(`  ⚠️  Patch ${index + 1} not applied (already patched or pattern not found): ${patch.description}`);
      }
    });
    
    // Write the patched content back to file
    fs.writeFileSync(filePath, content, 'utf8');
    console.log(`✅ ${fileName} patched successfully (${patchCount}/${patches.length} patches applied)\n`);
    return true;
    
  } catch (error) {
    console.error(`❌ Error patching ${fileName}:`, error.message);
    return false;
  }
}

function main() {
  console.log('🔧 Starting uniffi-bindgen-react-native patch script...\n');
  
  // Patches for ffi-converters.ts
  const ffiConvertersPatches = [
    {
      description: 'Change reader from private to public',
      from: 'private reader: (view: DataView) => T,',
      to: 'public reader: (view: DataView) => T,'
    },
    {
      description: 'Change writer from private to public', 
      from: 'private writer: (view: DataView, value: T) => void,',
      to: 'public writer: (view: DataView, value: T) => void,'
    },
    {
      description: 'Change byteSize from private to public',
      from: 'private byteSize: number,',
      to: 'public byteSize: number,'
    },
    {
      description: 'Add ArrayBuffer type assertion (return ab)',
      from: 'return ab;',
      to: 'return ab as ArrayBuffer;'
    },
    {
      description: 'Add ArrayBuffer type assertion (slice)',
      from: 'return ab.slice(start, end);',
      to: 'return ab.slice(start, end) as ArrayBuffer;'
    },
    {
      description: 'Add ArrayBuffer type assertion (readArrayBuffer)',
      from: 'return from.readArrayBuffer(length);',
      to: 'return from.readArrayBuffer(length) as ArrayBuffer;'
    }
  ];
  
  // Patches for ffi-types.ts
  const ffiTypesPatches = [
    {
      description: 'Add ArrayBuffer type assertion for buf.buffer',
      from: 'return new RustBuffer(buf.buffer);',
      to: 'return new RustBuffer(buf.buffer as ArrayBuffer);'
    }
  ];
  
  let success = true;
  
  // Patch ffi-converters.ts
  success &= patchFile(ffiConvertersPath, ffiConvertersPatches, 'ffi-converters.ts');
  
  // Patch ffi-types.ts
  success &= patchFile(ffiTypesPath, ffiTypesPatches, 'ffi-types.ts');
  
  if (success) {
    console.log('🎉 All patches applied successfully!');
    console.log('\n💡 Note: These patches will be lost when you reinstall node_modules.');
    console.log('   Consider using patch-package to make these changes persistent.');
  } else {
    console.log('❌ Some patches failed. Please check the output above.');
    process.exit(1);
  }
}

// Run the script
main();
