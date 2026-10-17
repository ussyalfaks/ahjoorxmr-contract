---
title: "docs: Fix clone URL and directory name in the README Quick Start"
labels: documentation, bug
assignees: ''
---

## Description
The README Quick Start section contains incorrect or outdated information for cloning the repository. The clone URL and/or directory name used in the instructions don't match the actual repository setup, which can confuse new users trying to get started with the project.

## Steps to Reproduce
1. Go to the main README.md
2. Look at the Quick Start or Getting Started section
3. Observe the clone command and directory name
4. Compare with the actual repository URL

## Expected Behavior
The README should provide:
- The correct `git clone` URL for the repository
- The correct directory name that matches the repository name
- Commands that work out-of-the-box for new users

Example format:
```bash
git clone https://github.com/[correct-org]/[correct-repo-name].git
cd [correct-repo-name]
```

## Actual Behavior
The current README contains a clone URL or directory name that doesn't match the actual repository, causing confusion and failed setup attempts for new contributors.

## Impact
- New users cannot successfully clone and set up the project using the README instructions
- Creates a poor first impression for potential contributors
- Wastes time as users need to figure out the correct commands independently

## Proposed Fix
Update the README.md file to:
1. Verify the correct repository URL
2. Update the `git clone` command with the correct URL
3. Ensure the `cd` command uses the correct directory name that matches the cloned repository
4. Test the commands to ensure they work end-to-end

## Additional Context
This is a quick documentation fix that will significantly improve the onboarding experience for new contributors and users of the project.

## Checklist
- [ ] Identify the correct clone URL
- [ ] Update the clone command in README.md
- [ ] Update the directory name in subsequent commands
- [ ] Verify all Quick Start commands work correctly
- [ ] Check if there are other references to the old URL/directory that need updating
