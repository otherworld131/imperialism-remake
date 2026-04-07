#!/usr/bin/env python3
"""
Batch simulation report generator.

Reads batch JSON output and generates a self-contained HTML report
with inline CSS and SVG charts.

Usage:
    python3 tools/batch_report/report.py results.json -o report.html
    python3 tools/batch_report/report.py results.json  # outputs to stdout
"""

import argparse
import json
import math
import sys
from collections import defaultdict
from html import escape

# Personality colors
PERSONALITY_COLORS = {
    "Aggressive": "#e74c3c",
    "Diplomatic": "#3498db",
    "Economic": "#2ecc71",
    "Balanced": "#95a5a6",
    "Human": "#f39c12",
}

DEFAULT_COLOR = "#8e44ad"


def get_color(personality):
    return PERSONALITY_COLORS.get(personality, DEFAULT_COLOR)


def compute_metric_stats(data, metric):
    """
    For each personality type, compute mean and stddev of a metric
    at each snapshot year, averaged across all games.

    Returns:
        {
            personality: {
                year: (mean, stddev),
                ...
            },
            ...
        }
    Also returns sorted list of years.
    """
    # Collect values: personality -> year -> [values across games]
    values = defaultdict(lambda: defaultdict(list))

    # Build a mapping from nation -> personality per game
    for game in data["games"]:
        personalities = game.get("personalities", {})
        snapshots = game.get("snapshots", {})
        for year_str, nation_data in snapshots.items():
            for nation, stats in nation_data.items():
                p = personalities.get(nation, "Unknown")
                val = stats.get(metric, 0)
                values[p][year_str].append(val)

    # Compute stats
    stats = {}
    all_years = set()
    for p, year_vals in values.items():
        stats[p] = {}
        for year_str, vals in year_vals.items():
            all_years.add(year_str)
            n = len(vals)
            mean = sum(vals) / n if n > 0 else 0
            if n > 1:
                variance = sum((v - mean) ** 2 for v in vals) / (n - 1)
                stddev = math.sqrt(variance)
            else:
                stddev = 0
            stats[p][year_str] = (mean, stddev)

    sorted_years = sorted(all_years, key=lambda y: int(y))
    return stats, sorted_years


def svg_line_chart(data, metric, title, width=720, height=400):
    """Generate an SVG line chart for a given metric."""
    stats, years = compute_metric_stats(data, metric)

    if not years or not stats:
        return '<p class="no-data">No data available for {}</p>'.format(escape(title))

    pad_left = 80
    pad_right = 30
    pad_top = 50
    pad_bottom = 60
    chart_w = width - pad_left - pad_right
    chart_h = height - pad_top - pad_bottom

    # Find global min/max across all personalities including stddev bands
    global_min = float("inf")
    global_max = float("-inf")
    for p, year_data in stats.items():
        for year_str, (mean, stddev) in year_data.items():
            lo = mean - stddev
            hi = mean + stddev
            if lo < global_min:
                global_min = lo
            if hi > global_max:
                global_max = hi

    if global_min == global_max:
        global_max = global_min + 1
    if global_min > 0:
        global_min = 0  # anchor at zero when all values positive

    y_range = global_max - global_min
    # Add 10% padding to top
    global_max += y_range * 0.1
    y_range = global_max - global_min

    def x_pos(i):
        if len(years) == 1:
            return pad_left + chart_w / 2
        return pad_left + (i / (len(years) - 1)) * chart_w

    def y_pos(val):
        if y_range == 0:
            return pad_top + chart_h / 2
        return pad_top + chart_h - ((val - global_min) / y_range) * chart_h

    svg_parts = []
    svg_parts.append(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" '
        'class="chart-svg" preserveAspectRatio="xMidYMid meet">'.format(width, height)
    )

    # Background
    svg_parts.append(
        '<rect x="0" y="0" width="{}" height="{}" fill="#fafafa" rx="8"/>'.format(
            width, height
        )
    )

    # Title
    svg_parts.append(
        '<text x="{}" y="30" text-anchor="middle" '
        'font-size="16" font-weight="bold" fill="#2c3e50">{}</text>'.format(
            width / 2, escape(title)
        )
    )

    # Grid lines and Y-axis labels
    num_grid = 5
    for i in range(num_grid + 1):
        val = global_min + (y_range * i / num_grid)
        y = y_pos(val)
        svg_parts.append(
            '<line x1="{}" y1="{}" x2="{}" y2="{}" '
            'stroke="#e0e0e0" stroke-width="1"/>'.format(pad_left, y, width - pad_right, y)
        )
        # Format label
        if abs(val) >= 10000:
            label = "{:.0f}k".format(val / 1000)
        elif abs(val) >= 1000:
            label = "{:.1f}k".format(val / 1000)
        else:
            label = "{:.0f}".format(val)
        svg_parts.append(
            '<text x="{}" y="{}" text-anchor="end" '
            'font-size="11" fill="#7f8c8d">{}</text>'.format(pad_left - 8, y + 4, label)
        )

    # X-axis labels
    for i, year_str in enumerate(years):
        x = x_pos(i)
        svg_parts.append(
            '<text x="{}" y="{}" text-anchor="middle" '
            'font-size="11" fill="#7f8c8d">{}</text>'.format(
                x, height - pad_bottom + 20, escape(year_str)
            )
        )
        # Vertical grid line
        svg_parts.append(
            '<line x1="{}" y1="{}" x2="{}" y2="{}" '
            'stroke="#e8e8e8" stroke-width="1"/>'.format(x, pad_top, x, pad_top + chart_h)
        )

    # Sort personalities for consistent ordering
    sorted_personalities = sorted(stats.keys(), key=lambda p: p)

    # Draw stddev bands first (behind lines)
    for p in sorted_personalities:
        year_data = stats[p]
        color = get_color(p)
        # Build upper and lower paths
        upper_points = []
        lower_points = []
        for i, year_str in enumerate(years):
            if year_str in year_data:
                mean, stddev = year_data[year_str]
                x = x_pos(i)
                upper_points.append((x, y_pos(mean + stddev)))
                lower_points.append((x, y_pos(mean - stddev)))

        if len(upper_points) >= 2:
            # Build polygon: upper path forward, lower path reversed
            path_d = "M {:.1f},{:.1f}".format(upper_points[0][0], upper_points[0][1])
            for pt in upper_points[1:]:
                path_d += " L {:.1f},{:.1f}".format(pt[0], pt[1])
            for pt in reversed(lower_points):
                path_d += " L {:.1f},{:.1f}".format(pt[0], pt[1])
            path_d += " Z"
            svg_parts.append(
                '<path d="{}" fill="{}" opacity="0.12"/>'.format(path_d, color)
            )

    # Draw lines
    for p in sorted_personalities:
        year_data = stats[p]
        color = get_color(p)
        points = []
        for i, year_str in enumerate(years):
            if year_str in year_data:
                mean, _stddev = year_data[year_str]
                x = x_pos(i)
                y = y_pos(mean)
                points.append((x, y))

        if len(points) >= 2:
            path_d = "M {:.1f},{:.1f}".format(points[0][0], points[0][1])
            for pt in points[1:]:
                path_d += " L {:.1f},{:.1f}".format(pt[0], pt[1])
            svg_parts.append(
                '<path d="{}" fill="none" stroke="{}" stroke-width="2.5" '
                'stroke-linecap="round" stroke-linejoin="round"/>'.format(path_d, color)
            )

        # Draw dots
        for px, py in points:
            svg_parts.append(
                '<circle cx="{:.1f}" cy="{:.1f}" r="3.5" fill="{}" stroke="white" '
                'stroke-width="1.5"/>'.format(px, py, color)
            )

    # Legend
    legend_x = pad_left + 10
    legend_y = pad_top + 10
    for idx, p in enumerate(sorted_personalities):
        color = get_color(p)
        lx = legend_x + (idx % 3) * 160
        ly = legend_y + (idx // 3) * 20
        svg_parts.append(
            '<rect x="{}" y="{}" width="14" height="14" rx="2" fill="{}"/>'.format(
                lx, ly - 11, color
            )
        )
        svg_parts.append(
            '<text x="{}" y="{}" font-size="12" fill="#2c3e50">{}</text>'.format(
                lx + 20, ly, escape(p)
            )
        )

    svg_parts.append("</svg>")
    return "\n".join(svg_parts)


def generate_summary_table(data):
    """Generate the summary statistics table with win-rate bar chart."""
    aggregate = data.get("aggregate", {})
    by_personality = aggregate.get("by_personality", {})

    if not by_personality:
        return "<p>No aggregate data available.</p>"

    rows = []
    sorted_personalities = sorted(by_personality.keys())
    for p in sorted_personalities:
        info = by_personality[p]
        color = get_color(p)
        games_played = info.get("games_played", 0)
        avg_score = info.get("avg_final_score", 0)
        stddev = info.get("stddev", 0)
        win_rate = info.get("win_rate", 0)
        win_pct = win_rate * 100

        bar_width = max(win_pct, 0)
        bar_svg = (
            '<svg width="120" height="20" class="bar-svg">'
            '<rect x="0" y="2" width="120" height="16" rx="3" fill="#ecf0f1"/>'
            '<rect x="0" y="2" width="{:.1f}" height="16" rx="3" fill="{}"/>'
            '<text x="60" y="14" text-anchor="middle" font-size="11" '
            'fill="#2c3e50" font-weight="bold">{:.1f}%</text>'
            "</svg>".format(bar_width * 1.2, color, win_pct)
        )

        rows.append(
            "<tr>"
            '<td><span class="color-dot" style="background:{}"></span>{}</td>'
            "<td>{}</td>"
            "<td>{:,.0f}</td>"
            "<td>{:,.0f}</td>"
            "<td>{}</td>"
            "</tr>".format(color, escape(p), games_played, avg_score, stddev, bar_svg)
        )

    return (
        '<table class="summary-table">'
        "<thead><tr>"
        "<th>Personality</th>"
        "<th>Games Played</th>"
        "<th>Avg Score</th>"
        "<th>Std Dev</th>"
        "<th>Win Rate</th>"
        "</tr></thead>"
        "<tbody>{}</tbody>"
        "</table>".format("".join(rows))
    )


def generate_per_game_table(data):
    """Generate collapsible per-game details."""
    games = data.get("games", [])
    if not games:
        return "<p>No game data available.</p>"

    parts = []
    for i, game in enumerate(games):
        seed = game.get("seed", "unknown")
        winner = game.get("winner", "N/A")
        personalities = game.get("personalities", {})
        final_scores = game.get("final_scores", {})
        wars = game.get("wars_declared", {})

        # Sort nations by final score descending
        sorted_nations = sorted(
            final_scores.keys(), key=lambda n: final_scores.get(n, 0), reverse=True
        )

        nation_rows = []
        for nation in sorted_nations:
            p = personalities.get(nation, "Unknown")
            score = final_scores.get(nation, 0)
            war_count = wars.get(nation, 0)
            color = get_color(p)
            is_winner = nation == winner
            winner_badge = ' <span class="winner-badge">WINNER</span>' if is_winner else ""
            nation_rows.append(
                "<tr{}>"
                "<td>{}{}</td>"
                '<td><span class="color-dot" style="background:{}"></span>{}</td>'
                "<td>{:,}</td>"
                "<td>{}</td>"
                "</tr>".format(
                    ' class="winner-row"' if is_winner else "",
                    escape(nation),
                    winner_badge,
                    color,
                    escape(p),
                    score,
                    war_count,
                )
            )

        parts.append(
            "<details>"
            "<summary>Game {} &mdash; Seed: <code>{}</code> &mdash; "
            'Winner: <strong>{}</strong></summary>'
            '<table class="game-table">'
            "<thead><tr>"
            "<th>Nation</th><th>Personality</th>"
            "<th>Final Score</th><th>Wars Declared</th>"
            "</tr></thead>"
            "<tbody>{}</tbody>"
            "</table>"
            "</details>".format(i + 1, escape(seed), escape(winner), "".join(nation_rows))
        )

    return "\n".join(parts)


def generate_html(data):
    """Generate the complete HTML report."""
    num_games = data.get("num_games", len(data.get("games", [])))
    difficulty = data.get("difficulty", "Unknown")

    # Build metric charts
    metrics = [
        ("provinces", "Provinces Over Time"),
        ("army_size", "Army Size Over Time"),
        ("treasury", "Treasury Over Time"),
        ("tech_count", "Tech Count Over Time"),
    ]

    chart_sections = []
    for metric_key, chart_title in metrics:
        svg = svg_line_chart(data, metric_key, chart_title)
        chart_sections.append(
            '<div class="card chart-card">'
            "{}"
            "</div>".format(svg)
        )

    charts_html = "\n".join(chart_sections)
    summary_table = generate_summary_table(data)
    per_game_html = generate_per_game_table(data)

    html = """\
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Batch Simulation Report</title>
<style>
*, *::before, *::after {{
    box-sizing: border-box;
}}
body {{
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
                 "Helvetica Neue", Arial, sans-serif;
    background: #f5f6fa;
    color: #2c3e50;
    line-height: 1.6;
}}
header {{
    background: linear-gradient(135deg, #2c3e50, #34495e);
    color: white;
    padding: 2rem 1rem;
    text-align: center;
}}
header h1 {{
    margin: 0 0 0.3rem 0;
    font-size: 1.8rem;
    font-weight: 700;
}}
header p {{
    margin: 0;
    opacity: 0.85;
    font-size: 1rem;
}}
.container {{
    max-width: 900px;
    margin: 0 auto;
    padding: 1.5rem 1rem;
}}
h2 {{
    font-size: 1.3rem;
    margin: 2rem 0 1rem 0;
    padding-bottom: 0.4rem;
    border-bottom: 2px solid #ecf0f1;
    color: #2c3e50;
}}
.card {{
    background: white;
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.07);
    padding: 1.2rem;
    margin-bottom: 1.2rem;
}}
.chart-card {{
    padding: 0.8rem;
    overflow-x: auto;
}}
.chart-svg {{
    width: 100%;
    height: auto;
    max-height: 420px;
}}
.bar-svg {{
    vertical-align: middle;
}}
.summary-table {{
    width: 100%;
    border-collapse: collapse;
    font-size: 0.95rem;
}}
.summary-table th {{
    background: #34495e;
    color: white;
    padding: 0.6rem 0.8rem;
    text-align: left;
    font-weight: 600;
}}
.summary-table th:first-child {{
    border-radius: 6px 0 0 0;
}}
.summary-table th:last-child {{
    border-radius: 0 6px 0 0;
}}
.summary-table td {{
    padding: 0.55rem 0.8rem;
    border-bottom: 1px solid #ecf0f1;
}}
.summary-table tbody tr:hover {{
    background: #f8f9fa;
}}
.color-dot {{
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    margin-right: 6px;
    vertical-align: middle;
}}
details {{
    background: white;
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.07);
    margin-bottom: 0.8rem;
    overflow: hidden;
}}
details summary {{
    padding: 0.8rem 1rem;
    cursor: pointer;
    font-size: 0.95rem;
    background: #fafafa;
    border-bottom: 1px solid #ecf0f1;
    user-select: none;
}}
details summary:hover {{
    background: #f0f0f0;
}}
details[open] summary {{
    font-weight: 600;
}}
.game-table {{
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
}}
.game-table th {{
    background: #ecf0f1;
    padding: 0.5rem 0.7rem;
    text-align: left;
    font-weight: 600;
}}
.game-table td {{
    padding: 0.45rem 0.7rem;
    border-bottom: 1px solid #f0f0f0;
}}
.game-table .winner-row {{
    background: #fef9e7;
}}
.winner-badge {{
    display: inline-block;
    background: #f1c40f;
    color: #2c3e50;
    font-size: 0.7rem;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    margin-left: 6px;
    vertical-align: middle;
}}
code {{
    background: #ecf0f1;
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 0.85em;
}}
.no-data {{
    color: #95a5a6;
    font-style: italic;
    text-align: center;
    padding: 2rem;
}}
.stat-grid {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 1rem;
    margin-bottom: 1.5rem;
}}
.stat-box {{
    background: white;
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.07);
    padding: 1rem;
    text-align: center;
}}
.stat-box .stat-value {{
    font-size: 1.8rem;
    font-weight: 700;
    color: #2c3e50;
}}
.stat-box .stat-label {{
    font-size: 0.85rem;
    color: #7f8c8d;
    margin-top: 0.2rem;
}}
footer {{
    text-align: center;
    padding: 2rem 1rem;
    color: #95a5a6;
    font-size: 0.85rem;
}}
@media (max-width: 600px) {{
    header h1 {{ font-size: 1.3rem; }}
    .container {{ padding: 1rem 0.5rem; }}
    .summary-table {{ font-size: 0.85rem; }}
}}
</style>
</head>
<body>

<header>
    <h1>Batch Simulation Report</h1>
    <p>{num_games} games &middot; {difficulty} difficulty</p>
</header>

<div class="container">

    <div class="stat-grid">
        <div class="stat-box">
            <div class="stat-value">{num_games}</div>
            <div class="stat-label">Games Simulated</div>
        </div>
        <div class="stat-box">
            <div class="stat-value">{difficulty}</div>
            <div class="stat-label">Difficulty</div>
        </div>
        <div class="stat-box">
            <div class="stat-value">{num_personalities}</div>
            <div class="stat-label">Personality Types</div>
        </div>
    </div>

    <h2>Summary by Personality</h2>
    <div class="card">
        {summary_table}
    </div>

    <h2>Metrics Over Time</h2>
    {charts_html}

    <h2>Per-Game Results</h2>
    {per_game_html}

</div>

<footer>
    Generated by Imperialism Remake Batch Report Tool
</footer>

</body>
</html>""".format(
        num_games=num_games,
        difficulty=escape(str(difficulty)),
        num_personalities=len(
            data.get("aggregate", {}).get("by_personality", {})
        ),
        summary_table=summary_table,
        charts_html=charts_html,
        per_game_html=per_game_html,
    )

    return html


def main():
    parser = argparse.ArgumentParser(
        description="Generate an HTML report from batch simulation JSON output."
    )
    parser.add_argument(
        "input",
        help="Path to the batch results JSON file",
    )
    parser.add_argument(
        "-o",
        "--output",
        default=None,
        help="Output HTML file path (default: stdout)",
    )
    args = parser.parse_args()

    with open(args.input, "r") as f:
        data = json.load(f)

    html = generate_html(data)

    if args.output:
        with open(args.output, "w") as f:
            f.write(html)
        print("Report written to {}".format(args.output), file=sys.stderr)
    else:
        sys.stdout.write(html)


if __name__ == "__main__":
    main()
