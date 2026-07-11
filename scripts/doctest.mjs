#!/usr/bin/env node
/**
 * doctest.mjs - Extract and validate code samples from markdown documentation.
 *
 * Finds all .md files in cookbook/ and docs/ directories, extracts fenced code blocks,
 * validates JSON samples and checks syntax for TypeScript/JavaScript samples.
 *
 * No dependencies - uses only Node.js built-in modules.
 */

import fs from 'fs';
import path from 'path';
import { execSync } from 'child_process';
import os from 'os';

const REPO_ROOT = process.cwd();
const COOKBOOK_DIR = path.join(REPO_ROOT, 'cookbook');
const DOCS_DIR = path.join(REPO_ROOT, 'docs');
const TEMP_DIR = os.tmpdir();

let stats = {
  filesProcessed: 0,
  blocksFound: 0,
  blocksSkipped: 0,
  jsonValid: 0,
  jsonInvalid: 0,
  syntaxValid: 0,
  syntaxInvalid: 0,
  errors: []
};

/**
 * Recursively find all .md files in a directory
 */
function findMarkdownFiles(dir) {
  const files = [];

  if (!fs.existsSync(dir)) {
    return files;
  }

  const entries = fs.readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...findMarkdownFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      files.push(fullPath);
    }
  }

  return files;
}

/**
 * Extract fenced code blocks from markdown content
 * Returns array of { language, code, hasNoTest, lineNumber }
 */
function extractCodeBlocks(content) {
  const blocks = [];
  const lines = content.split('\n');

  let inBlock = false;
  let blockStart = 0;
  let blockLanguage = '';
  let blockCode = [];
  let blockHasNoTest = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Opening fence
    const openMatch = line.match(/^```(\w+)?/);
    if (openMatch && !inBlock) {
      inBlock = true;
      blockStart = i;
      blockLanguage = openMatch[1] || '';
      blockCode = [];

      // Check for no-test marker on same line or previous line
      blockHasNoTest =
        line.includes('no-test') ||
        (i > 0 && lines[i - 1].includes('no-test'));

      continue;
    }

    // Closing fence
    if (line.match(/^```$/) && inBlock) {
      inBlock = false;
      blocks.push({
        language: blockLanguage,
        code: blockCode.join('\n'),
        hasNoTest: blockHasNoTest,
        lineNumber: blockStart + 1
      });
      continue;
    }

    // Content within block
    if (inBlock) {
      blockCode.push(line);
    }
  }

  return blocks;
}

/**
 * Validate JSON code block
 */
function validateJSON(code, filePath, lineNumber) {
  try {
    JSON.parse(code);
    stats.jsonValid++;
    return { valid: true };
  } catch (error) {
    stats.jsonInvalid++;
    const msg = `JSON parse error in ${filePath}:${lineNumber}: ${error.message}`;
    stats.errors.push(msg);
    return { valid: false, error: msg };
  }
}

/**
 * Check syntax for TypeScript/JavaScript code block
 */
function checkSyntax(code, language, filePath, lineNumber) {
  try {
    const tempFile = path.join(TEMP_DIR, `doctest-${Date.now()}-${Math.random().toString(36).substr(2, 9)}.js`);

    // Write to temp file
    fs.writeFileSync(tempFile, code, 'utf8');

    try {
      // Use node --check to validate syntax
      execSync(`node --check "${tempFile}"`, { stdio: 'pipe' });
      stats.syntaxValid++;
      fs.unlinkSync(tempFile);
      return { valid: true };
    } catch (error) {
      stats.syntaxInvalid++;
      const msg = `${language} syntax error in ${filePath}:${lineNumber}: ${error.message}`;
      stats.errors.push(msg);
      fs.unlinkSync(tempFile);
      return { valid: false, error: msg };
    }
  } catch (error) {
    const msg = `Error checking ${language} syntax in ${filePath}:${lineNumber}: ${error.message}`;
    stats.errors.push(msg);
    return { valid: false, error: msg };
  }
}

/**
 * Process a single markdown file
 */
function processFile(filePath) {
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    const blocks = extractCodeBlocks(content);

    if (blocks.length > 0) {
      stats.filesProcessed++;
    }

    for (const block of blocks) {
      stats.blocksFound++;

      // Skip blocks marked with no-test
      if (block.hasNoTest) {
        stats.blocksSkipped++;
        continue;
      }

      const code = block.code.trim();
      if (!code) {
        stats.blocksSkipped++;
        continue;
      }

      // Validate based on language
      if (block.language === 'json') {
        validateJSON(code, filePath, block.lineNumber);
      } else if (block.language === 'ts' || block.language === 'typescript') {
        checkSyntax(code, 'TypeScript', filePath, block.lineNumber);
      } else if (block.language === 'js' || block.language === 'javascript') {
        checkSyntax(code, 'JavaScript', filePath, block.lineNumber);
      } else {
        // Skip other languages
        stats.blocksSkipped++;
      }
    }
  } catch (error) {
    const msg = `Error processing ${filePath}: ${error.message}`;
    stats.errors.push(msg);
  }
}

/**
 * Main entry point
 */
function main() {
  console.log('Running doctest on cookbook and docs...\n');

  // Find all markdown files
  const files = [
    ...findMarkdownFiles(COOKBOOK_DIR),
    ...findMarkdownFiles(DOCS_DIR)
  ];

  if (files.length === 0) {
    console.log('No markdown files found in cookbook/ or docs/ directories.');
    process.exit(0);
  }

  console.log(`Found ${files.length} markdown files. Processing...\n`);

  // Process each file
  for (const filePath of files) {
    processFile(filePath);
  }

  // Print summary
  console.log('\n========== DOCTEST SUMMARY ==========\n');
  console.log(`Files processed:     ${stats.filesProcessed}`);
  console.log(`Code blocks found:   ${stats.blocksFound}`);
  console.log(`  - Skipped:         ${stats.blocksSkipped}`);
  console.log(`  - Validated:       ${stats.blocksFound - stats.blocksSkipped}`);
  console.log();
  console.log(`JSON blocks valid:   ${stats.jsonValid}`);
  console.log(`JSON blocks invalid: ${stats.jsonInvalid}`);
  console.log();
  console.log(`Syntax checks pass:  ${stats.syntaxValid}`);
  console.log(`Syntax checks fail:  ${stats.syntaxInvalid}`);

  if (stats.errors.length > 0) {
    console.log(`\n========== ERRORS ==========\n`);
    for (const error of stats.errors) {
      console.log(`  ERROR: ${error}`);
    }
    console.log();
  }

  // Exit with appropriate code
  const totalIssues = stats.jsonInvalid + stats.syntaxInvalid;
  if (totalIssues > 0) {
    console.log(`RESULT: ${totalIssues} validation failure(s). Run failed.\n`);
    process.exit(1);
  } else {
    console.log('RESULT: All code samples validated successfully.\n');
    process.exit(0);
  }
}

main();
