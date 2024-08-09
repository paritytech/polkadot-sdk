from github import Github
import re
import os
from datetime import date

g = Github(os.getenv("GH_TOKEN"))

# Regex pattern to match wish format:
wish_pattern = re.compile(
    r"I wish for:? (https://github\.com/([a-zA-Z0-9_.-]+)/([a-zA-Z0-9_.-]+)/(issues|pull)/(\d+))"
)

wishlist_issue = g.get_repo(os.getenv("WISHLIST_REPOSITORY")).get_issue(
    int(os.getenv("WISHLIST_ISSUE_NUMBER"))
)
new_leaderboard = (
    "| Feature Request | Summary | Votes | Status |\n| --- | --- | --- | --- |\n"
)
wishes = {}
issue_details = {}

for comment in wishlist_issue.get_comments():
    # in the comment body, if there is a string `#(\d)`, replace it with
    # https://github.com/paritytech/polkadot-sdk/issues/(number)
    updated_body = re.sub(
        r"#(\d+)", r"https://github.com/paritytech/polkadot-sdk/issues/\1", comment.body
    )

    matches = wish_pattern.findall(updated_body)
    for match in matches:
        url, org, repo_name, _, issue_id = match
        issue_key = (url, org, repo_name, issue_id)
        if issue_key not in wishes:
            wishes[issue_key] = []

        # Get the author and upvoters of the wish comment.
        wishes[issue_key].append(comment.user.id)
        wishes[issue_key].extend(
            [
                reaction.user.id
                for reaction in comment.get_reactions()
                if reaction.content in ["+1", "heart", "rocket"]
            ]
        )

        # Get upvoters of the desired issue.
        desired_issue = g.get_repo(f"{org}/{repo_name}").get_issue(int(issue_id))
        wishes[issue_key].extend(
            [
                reaction.user.id
                for reaction in desired_issue.get_reactions()
                if reaction.content in ["+1", "heart", "rocket"]
            ]
        )
        issue_details[url] = [
            desired_issue.title,
            "👾 Open" if desired_issue.state == "open" else "✅Closed",
        ]

# Count unique wishes - the author of the wish, upvoters of the wish, and upvoters of the desired issue.
for key in wishes:
    wishes[key] = len(list(set(wishes[key])))

# Sort wishes by count and add to the markdown table
sorted_wishes = sorted(wishes.items(), key=lambda x: x[1], reverse=True)
for (url, _, _, _), count in sorted_wishes:
    [summary, status] = issue_details.get(url, "No summary available")
    new_leaderboard += f"| {url} | {summary} | {count} | {status} |\n"
new_leaderboard += f"\n> Last updated: {date.today().strftime('%Y-%m-%d')}\n"
print(new_leaderboard)

new_content = re.sub(
    r"(\| Feature Request \|)(.*?)(> Last updated:)(.*?\n)",
    new_leaderboard,
    wishlist_issue.body,
    flags=re.DOTALL,
)

wishlist_issue.edit(body=new_content)
