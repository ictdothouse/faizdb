#!/usr/bin/env python3
"""
FaizDB-Mind Research Paper PDF Builder
Converts docs/FAIZDB_MIND_RESEARCH_PAPER.md into a high-grade publication-ready PDF
complete with rendered KaTeX formulas, academic typography, and booktabs tables.
"""

import json
import os
import subprocess
import sys

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    md_path = os.path.join(repo_root, "docs", "FAIZDB_MIND_RESEARCH_PAPER.md")
    html_path = os.path.join(repo_root, "docs", "FAIZDB_MIND_RESEARCH_PAPER.html")
    pdf_path = os.path.join(repo_root, "docs", "FAIZDB_MIND_RESEARCH_PAPER.pdf")

    if not os.path.exists(md_path):
        print(f"Error: Markdown file not found at {md_path}")
        sys.exit(1)

    with open(md_path, "r", encoding="utf-8") as f:
        markdown_text = f.read()

    md_json = json.dumps(markdown_text)

    html_template = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>FaizDB-Mind: Zero-Backpropagation Edge Intelligence</title>
  
  <!-- Fonts -->
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Inter:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@400;500;600&family=Source+Serif+4:ital,opsz,wght@0,8..60,400;0,8..60,600;0,8..60,700;1,8..60,400&display=swap" rel="stylesheet">

  <!-- KaTeX for Mathematics -->
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css">
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/contrib/auto-render.min.js"></script>

  <!-- Marked for Markdown Parsing -->
  <script src="https://cdn.jsdelivr.net/npm/marked@12.0.0/marked.min.js"></script>

  <style>
    @page {{
      size: A4 portrait;
      margin: 18mm 16mm 18mm 16mm;
      @bottom-right {{
        content: "Page " counter(page);
        font-family: 'Inter', sans-serif;
        font-size: 8pt;
        color: #64748b;
      }}
      @bottom-left {{
        content: "FaizDB-Mind: Zero-Backpropagation Cognitive Architecture";
        font-family: 'Inter', sans-serif;
        font-size: 8pt;
        color: #64748b;
      }}
      @top-right {{
        content: "ICT HOUSE Systems Research (September 2026)";
        font-family: 'Inter', sans-serif;
        font-size: 7.5pt;
        color: #94a3b8;
      }}
    }}

    *, *::before, *::after {{
      box-sizing: border-box;
    }}

    body {{
      font-family: 'Source Serif 4', 'Charter', Georgia, serif;
      font-size: 9.8pt;
      line-height: 1.55;
      color: #0f172a;
      background-color: #ffffff;
      margin: 0;
      padding: 0;
      -webkit-font-smoothing: antialiased;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }}

    /* Title Block */
    h1 {{
      font-family: 'Inter', sans-serif;
      font-size: 19pt;
      font-weight: 700;
      line-height: 1.25;
      color: #0f172a;
      text-align: center;
      margin-top: 0;
      margin-bottom: 10px;
      letter-spacing: -0.02em;
    }}

    .meta-block {{
      text-align: center;
      font-family: 'Inter', sans-serif;
      font-size: 9pt;
      color: #475569;
      margin-bottom: 22px;
      line-height: 1.45;
    }}

    .meta-block strong {{
      color: #0f172a;
      font-weight: 600;
    }}

    /* Abstract Block */
    .abstract-box {{
      background-color: #f8fafc;
      border-top: 1.5px solid #cbd5e1;
      border-bottom: 1.5px solid #cbd5e1;
      padding: 12px 18px;
      margin: 16px 0 22px 0;
      font-size: 9.1pt;
      line-height: 1.52;
      text-align: justify;
    }}

    .abstract-box strong:first-child {{
      font-family: 'Inter', sans-serif;
      font-size: 9.2pt;
      letter-spacing: 0.05em;
      text-transform: uppercase;
      color: #1e293b;
      margin-right: 6px;
    }}

    /* Headings */
    h2 {{
      font-family: 'Inter', sans-serif;
      font-size: 13pt;
      font-weight: 700;
      color: #0f172a;
      border-bottom: 1.2px solid #e2e8f0;
      padding-bottom: 4px;
      margin-top: 26px;
      margin-bottom: 10px;
      letter-spacing: -0.01em;
      break-after: avoid;
    }}

    h3 {{
      font-family: 'Inter', sans-serif;
      font-size: 10.8pt;
      font-weight: 600;
      color: #1e293b;
      margin-top: 18px;
      margin-bottom: 6px;
      break-after: avoid;
    }}

    h4 {{
      font-family: 'Inter', sans-serif;
      font-size: 9.8pt;
      font-weight: 600;
      color: #334155;
      margin-top: 14px;
      margin-bottom: 4px;
      break-after: avoid;
    }}

    p {{
      margin-top: 0;
      margin-bottom: 9px;
      text-align: justify;
    }}

    /* Tables */
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: 'Inter', sans-serif;
      font-size: 8.2pt;
      margin: 16px 0 18px 0;
      break-inside: avoid;
      border-top: 2px solid #0f172a;
      border-bottom: 2px solid #0f172a;
    }}

    th {{
      background-color: #f1f5f9;
      color: #0f172a;
      font-weight: 600;
      text-align: left;
      padding: 6px 9px;
      border-bottom: 1.5px solid #334155;
    }}

    td {{
      padding: 5px 9px;
      border-bottom: 1px solid #e2e8f0;
      vertical-align: top;
      line-height: 1.38;
    }}

    tr:last-child td {{
      border-bottom: none;
    }}

    tr:nth-child(even) td {{
      background-color: #f8fafc;
    }}

    /* Code & ASCII blocks */
    code {{
      font-family: 'JetBrains Mono', 'Consolas', monospace;
      font-size: 8.2pt;
      background-color: #f1f5f9;
      padding: 1px 4px;
      border-radius: 3px;
      color: #0369a1;
    }}

    pre {{
      background-color: #0f172a;
      color: #f8fafc;
      padding: 10px 14px;
      border-radius: 6px;
      overflow-x: auto;
      font-size: 7.8pt;
      line-height: 1.4;
      margin: 12px 0;
      break-inside: avoid;
    }}

    pre code {{
      background: none;
      padding: 0;
      color: inherit;
      font-size: inherit;
    }}

    /* Mathematics */
    .katex-display {{
      margin: 12px 0 !important;
      overflow-x: auto;
      overflow-y: hidden;
      break-inside: avoid;
      font-size: 1.02em;
    }}

    /* Lists */
    ul, ol {{
      margin-top: 0;
      margin-bottom: 9px;
      padding-left: 18px;
    }}

    li {{
      margin-bottom: 3px;
    }}

    hr {{
      border: none;
      border-top: 1px solid #e2e8f0;
      margin: 20px 0;
    }}

    /* Print optimizations */
    @media print {{
      body {{
        background: transparent;
      }}
      h1, h2, h3, h4 {{
        break-after: avoid;
      }}
      table, pre, .abstract-box {{
        break-inside: avoid;
      }}
    }}
  </style>
</head>
<body>

  <div id="content"></div>

  <script>
    const rawMarkdown = {md_json};

    marked.setOptions({{
      gfm: true,
      breaks: false,
    }});

    // Protect math from marked parser
    const mathStore = [];
    let processedMarkdown = rawMarkdown.replace(/\\$\\$([\\s\\S]*?)\\$\\$/g, (match, eq) => {{
      const token = `@@MATH_BLOCK_${{mathStore.length}}@@`;
      mathStore.push({{ token, eq, display: true }});
      return token;
    }});

    processedMarkdown = processedMarkdown.replace(/\\$([^\\$\\n]+?)\\$/g, (match, eq) => {{
      const token = `@@MATH_INLINE_${{mathStore.length}}@@`;
      mathStore.push({{ token, eq, display: false }});
      return token;
    }});

    let htmlContent = marked.parse(processedMarkdown);

    mathStore.forEach(item => {{
      const replacement = item.display ? `$$${{item.eq}}$$` : `$${{item.eq}}$`;
      htmlContent = htmlContent.split(item.token).join(replacement);
    }});

    const container = document.getElementById('content');
    container.innerHTML = htmlContent;

    // Format title & metadata block
    const firstH1 = container.querySelector('h1');
    if (firstH1) {{
      let sibling = firstH1.nextElementSibling;
      const metaContainer = document.createElement('div');
      metaContainer.className = 'meta-block';

      while (sibling && sibling.tagName !== 'H2' && sibling.tagName !== 'HR') {{
        const next = sibling.nextElementSibling;
        metaContainer.appendChild(sibling);
        sibling = next;
      }}

      firstH1.parentNode.insertBefore(metaContainer, sibling);
    }}

    // Wrap Abstract
    const allH2 = container.querySelectorAll('h2');
    allH2.forEach(h2 => {{
      if (h2.textContent.trim().toLowerCase() === 'abstract') {{
        const abstractBox = document.createElement('div');
        abstractBox.className = 'abstract-box';
        
        let node = h2.nextElementSibling;
        while (node && node.tagName !== 'H2' && node.tagName !== 'HR') {{
          const nextNode = node.nextElementSibling;
          abstractBox.appendChild(node);
          node = nextNode;
        }}
        h2.parentNode.insertBefore(abstractBox, node);
        h2.remove();
      }}
    }});

    // Render KaTeX Mathematics
    renderMathInElement(document.body, {{
      delimiters: [
        {{ left: '$$', right: '$$', display: true }},
        {{ left: '$', right: '$', display: false }}
      ],
      throwOnError: false
    }});
  </script>
</body>
</html>
"""

    with open(html_path, "w", encoding="utf-8") as f:
        f.write(html_template)

    print(f"Generated HTML template at: {html_path}")

    # Edge Headless PDF Generation
    edge_paths = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        "msedge.exe"
    ]
    edge_exe = None
    for p in edge_paths:
        if os.path.exists(p):
            edge_exe = p
            break

    if not edge_exe:
        edge_exe = "msedge.exe"

    print(f"Using browser executable: {edge_exe}")
    print("Executing headless print-to-pdf...")

    args = [
        edge_exe,
        "--headless=new",
        "--disable-gpu",
        "--run-all-compositor-stages-before-draw",
        "--virtual-time-budget=9000",
        "--no-pdf-header-footer",
        f"--print-to-pdf={pdf_path}",
        html_path
    ]

    res = subprocess.run(args, capture_output=True, text=True)
    
    if os.path.exists(pdf_path) and os.path.getsize(pdf_path) > 1000:
        pdf_size_kb = os.path.getsize(pdf_path) / 1024
        print(f"SUCCESS: Research Paper PDF created successfully!")
        print(f"File Path: {pdf_path}")
        print(f"Size: {pdf_size_kb:.1f} KB")
    else:
        print("Failed to generate PDF. Stderr:", res.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
