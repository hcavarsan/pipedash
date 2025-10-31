#!/bin/bash

set -e

echo "GitHub Repository History Cleanup"
echo "================================="
echo
echo "⚠️  WARNING: This will completely erase your repository history!"
echo "Make sure you have a backup of any important code."
echo
read -p "Are you absolutely sure you want to continue? (yes/no): " confirmation

if [ "$confirmation" != "yes" ]; then
    echo "Aborted."
    exit 1
fi

# Store current branch name
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

# Step 1: Create a new orphan branch
echo
echo "Step 1: Creating new clean branch..."
git checkout --orphan clean-history

# Step 2: Add all files (except sensitive ones)
echo
echo "Step 2: Adding all files to new commit..."
git add -A

# Step 3: Commit with a clean history
echo
echo "Step 3: Creating initial commit..."
git commit -m "Initial commit - clean history"

# Step 4: Delete the old branch
echo
echo "Step 4: Deleting old branch..."
git branch -D "$CURRENT_BRANCH"

# Step 5: Rename current branch to main
echo
echo "Step 5: Renaming branch to main..."
git branch -m main

# Step 6: Force push to GitHub
echo
echo "Step 6: Force pushing to GitHub..."
echo "This will completely replace the remote repository!"
read -p "Press Enter to continue..."

git push -f origin main

# Step 7: Clean up any remaining sensitive files locally
echo
echo "Step 7: Cleaning up local repository..."

# Remove any tags
git tag -l | xargs -n 1 git push --delete origin 2>/dev/null || true
git tag -l | xargs -n 1 git tag -d 2>/dev/null || true

# Garbage collect
git reflog expire --expire=now --all
git gc --prune=now --aggressive

echo
echo "✅ Repository history has been completely erased!"
echo
echo "Next steps:"
echo "1. Verify on GitHub that the history is clean"
echo "2. Make sure .gitignore includes all sensitive files:"
echo "   - *.p12"
echo "   - *.cer"
echo "   - certificate-base64.txt"
echo "   - .env"
echo "   - **/DeveloperID_*"
echo "3. Continue development with the clean repository"
echo
echo "Your repository now has only one commit with the current state of your code."
