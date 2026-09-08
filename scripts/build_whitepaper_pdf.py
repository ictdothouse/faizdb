#!/usr/bin/env python3
"""
FaizDB Whitepaper PDF Builder
Converts docs/FAIZDB_ARCHITECTURE_WHITEPAPER.md into a high-grade publication-ready PDF
complete with rendered Mermaid diagrams, KaTeX formulas, and academic typography.
"""

import json
import os
import subprocess
import sys
import time

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    md_path = os.path.join(repo_root, "docs", "FAIZDB_ARCHITECTURE_WHITEPAPER.md")
    html_path = os.path.join(repo_root, "docs", "FAIZDB_ARCHITECTURE_WHITEPAPER.html")
    pdf_path = os.path.join(repo_root, "docs", "FAIZDB_ARCHITECTURE_WHITEPAPER.pdf")

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
  <title>FaizDB: Technical Architecture Whitepaper</title>
  
  <!-- Fonts -->
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Cinzel:wght@700&family=Inter:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@400;500;600&family=Source+Serif+4:ital,opsz,wght@0,8..60,400;0,8..60,600;0,8..60,700;1,8..60,400&display=swap" rel="stylesheet">

  <!-- KaTeX for Mathematics -->
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css">
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/contrib/auto-render.min.js"></script>

  <!-- Marked for Markdown Parsing -->
  <script src="https://cdn.jsdelivr.net/npm/marked@12.0.0/marked.min.js"></script>

  <!-- Mermaid for Architecture Diagrams -->
  <script src="https://cdn.jsdelivr.net/npm/mermaid@10.9.0/dist/mermaid.min.js"></script>

  <style>
    @page {{
      size: A4 portrait;
      margin: 20mm 18mm 20mm 18mm;
      @bottom-right {{
        content: "Page " counter(page);
        font-family: 'Inter', sans-serif;
        font-size: 8.5pt;
        color: #718096;
      }}
      @bottom-left {{
        content: "FaizDB Architecture Whitepaper v1.0";
        font-family: 'Inter', sans-serif;
        font-size: 8.5pt;
        color: #718096;
      }}
      @top-right {{
        content: "Ahmad Faiz (September 2026)";
        font-family: 'Inter', sans-serif;
        font-size: 8pt;
        color: #a0aec0;
      }}
    }}

    *, *::before, *::after {{
      box-sizing: border-box;
    }}

    body {{
      font-family: 'Source Serif 4', 'Charter', Georgia, serif;
      font-size: 10.2pt;
      line-height: 1.58;
      color: #1a202c;
      background-color: #ffffff;
      margin: 0;
      padding: 0;
      -webkit-font-smoothing: antialiased;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }}

    /* Paper Header / Title Block */
    h1 {{
      font-family: 'Inter', sans-serif;
      font-size: 20pt;
      font-weight: 700;
      line-height: 1.25;
      color: #0f172a;
      text-align: center;
      margin-top: 0;
      margin-bottom: 8px;
      letter-spacing: -0.02em;
    }}

    /* Author & Metadata Block */
    .meta-block {{
      text-align: center;
      font-family: 'Inter', sans-serif;
      font-size: 9.5pt;
      color: #475569;
      margin-bottom: 24px;
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
      padding: 14px 18px;
      margin: 18px 0 24px 0;
      font-size: 9.3pt;
      line-height: 1.54;
      text-align: justify;
    }}

    .abstract-box strong:first-child {{
      font-family: 'Inter', sans-serif;
      font-size: 9.5pt;
      letter-spacing: 0.05em;
      text-transform: uppercase;
      color: #1e293b;
      margin-right: 6px;
    }}

    /* Headings */
    h2 {{
      font-family: 'Inter', sans-serif;
      font-size: 13.5pt;
      font-weight: 700;
      color: #0f172a;
      border-bottom: 1.2px solid #e2e8f0;
      padding-bottom: 4px;
      margin-top: 30px;
      margin-bottom: 12px;
      letter-spacing: -0.01em;
      break-after: avoid;
    }}

    h3 {{
      font-family: 'Inter', sans-serif;
      font-size: 11.2pt;
      font-weight: 600;
      color: #1e293b;
      margin-top: 20px;
      margin-bottom: 8px;
      break-after: avoid;
    }}

    h4 {{
      font-family: 'Inter', sans-serif;
      font-size: 10pt;
      font-weight: 600;
      color: #334155;
      margin-top: 14px;
      margin-bottom: 6px;
      break-after: avoid;
    }}

    p {{
      margin-top: 0;
      margin-bottom: 10px;
      text-align: justify;
    }}

    /* Tables (Academic Booktabs Style) */
    table {{
      width: 100%;
      border-collapse: collapse;
      font-family: 'Inter', sans-serif;
      font-size: 8.5pt;
      margin: 18px 0 22px 0;
      break-inside: avoid;
      border-top: 2px solid #0f172a;
      border-bottom: 2px solid #0f172a;
    }}

    th {{
      background-color: #f1f5f9;
      color: #0f172a;
      font-weight: 600;
      text-align: left;
      padding: 7px 10px;
      border-bottom: 1.5px solid #334155;
    }}

    td {{
      padding: 6px 10px;
      border-bottom: 1px solid #e2e8f0;
      vertical-align: top;
      line-height: 1.4;
    }}

    tr:last-child td {{
      border-bottom: none;
    }}

    tr:nth-child(even) td {{
      background-color: #f8fafc;
    }}

    /* Code & Preformatted */
    code {{
      font-family: 'JetBrains Mono', 'Consolas', monospace;
      font-size: 8.5pt;
      background-color: #f1f5f9;
      padding: 1.5px 4.5px;
      border-radius: 3px;
      color: #0969da;
    }}

    pre {{
      background-color: #0d1117;
      color: #e6edf3;
      padding: 12px 16px;
      border-radius: 6px;
      overflow-x: auto;
      font-size: 8.2pt;
      line-height: 1.45;
      margin: 14px 0;
      break-inside: avoid;
      box-shadow: inset 0 0 0 1px #30363d;
    }}

    pre code {{
      background: none;
      padding: 0;
      color: inherit;
      font-size: inherit;
    }}

    /* Diagrams (Mermaid) */
    .diagram-container {{
      background-color: #ffffff;
      border: 1px solid #e2e8f0;
      border-radius: 6px;
      padding: 12px 10px;
      margin: 16px 0;
      text-align: center;
      break-inside: avoid;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
    }}

    .diagram-container svg {{
      max-width: 100% !important;
      max-height: 380px !important;
      height: auto !important;
      margin: 0 auto;
    }}

    .figure-caption {{
      font-family: 'Inter', sans-serif;
      font-size: 8.5pt;
      font-weight: 600;
      color: #334155;
      text-align: center;
      margin-top: 8px;
      margin-bottom: 0;
    }}

    /* Mathematics */
    .katex-display {{
      margin: 14px 0 !important;
      overflow-x: auto;
      overflow-y: hidden;
      break-inside: avoid;
      font-size: 1.05em;
    }}

    /* Lists */
    ul, ol {{
      margin-top: 0;
      margin-bottom: 10px;
      padding-left: 20px;
    }}

    li {{
      margin-bottom: 4px;
    }}

    /* Horizontal Rules */
    hr {{
      border: none;
      border-top: 1px solid #e2e8f0;
      margin: 24px 0;
    }}

    /* Blockquotes */
    blockquote {{
      margin: 14px 0;
      padding: 10px 16px;
      background-color: #f8fafc;
      border-left: 3.5px solid #3b82f6;
      color: #334155;
      font-style: italic;
      break-inside: avoid;
    }}

    /* Link styling */
    a {{
      color: #0284c7;
      text-decoration: none;
    }}

    a:hover {{
      text-decoration: underline;
    }}

    /* Print optimizations */
    @media print {{
      body {{
        background: transparent;
      }}
      h1, h2, h3, h4 {{
        break-after: avoid;
      }}
      .diagram-container, table, pre, .abstract-box, blockquote {{
        break-inside: avoid;
      }}
    }}
  </style>
</head>
<body>

  <div id="content"></div>

  <script>
    const rawMarkdown = {md_json};

    // Configure marked
    marked.setOptions({{
      gfm: true,
      breaks: false,
    }});

    // Protect display and inline math from Marked markdown parsing
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

    // Parse Markdown
    let htmlContent = marked.parse(processedMarkdown);

    // Restore Math expressions
    mathStore.forEach(item => {{
      const replacement = item.display ? `$$${{item.eq}}$$` : `$${{item.eq}}$`;
      htmlContent = htmlContent.split(item.token).join(replacement);
    }});

    const container = document.getElementById('content');
    container.innerHTML = htmlContent;

    // Process Title & Metadata header
    const firstH1 = container.querySelector('h1');
    if (firstH1) {{
      firstH1.className = 'paper-title';
      
      // Collect metadata lines immediately following H1
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

    // Wrap Abstract in styled box
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
        h2.remove(); // Remove raw H2 Abstract header to integrate seamlessly
      }}
    }});

    // Explicit academic figure captions
    const figureCaptions = [
      "Figure 1: Architectural Comparison: Fragile Polyglot Persistence Sprawl vs. FaizDB Unified In-Memory Convergence Engine",
      "Figure 2: FaizDB Modular 7-Layer Architecture and Monorepo Internal Pipeline Topology",
      "Figure 3: End-to-End Write Transaction & Durability Lifecycle (Gateway -> WAL fsync -> SkipList MemTable -> SSTable Compaction)",
      "Figure 4: Four-Tier Read Path Hierarchy: Active MemTable, Immutable MemTable, ARC Block Cache, and Bloom Filter",
      "Figure 5: Single-Pass Converged Multi-Model Query Pipeline (Graph Topo Filter -> SIMD Vector ANN -> Document Projection)",
      "Figure 6: Concurrent 5-Way Wire Protocol Multiplexer Gateway and Unified Zero-Trust Security Enforcement",
      "Figure 7: Distributed Consistency Duality: CP Raft Consensus vs. Multi-Region Active-Active AP CRDT WAN Mesh",
      "Figure 8: Quantitative Resource Efficiency Comparison: Executable Binary Size and Idle Memory Footprint (VmRSS)"
    ];

    // Identify and replace Mermaid code blocks
    let diagramIndex = 0;
    const codeBlocks = container.querySelectorAll('pre code.language-mermaid');
    codeBlocks.forEach(code => {{
      const pre = code.parentElement;
      const rawDiagram = code.textContent.trim();

      const diagramDiv = document.createElement('div');
      diagramDiv.className = 'diagram-container';

      const mermaidDiv = document.createElement('div');
      mermaidDiv.className = 'mermaid';
      mermaidDiv.textContent = rawDiagram;

      const caption = document.createElement('div');
      caption.className = 'figure-caption';
      caption.textContent = figureCaptions[diagramIndex] || `Figure ${{diagramIndex + 1}}: Architectural Specification Diagram`;
      diagramIndex++;

      diagramDiv.appendChild(mermaidDiv);
      diagramDiv.appendChild(caption);

      pre.parentNode.replaceChild(diagramDiv, pre);
    }});

    // Initialize & render Mermaid
    mermaid.initialize({{
      startOnLoad: false,
      theme: 'neutral',
      fontFamily: 'Inter, sans-serif',
      fontSize: 11,
      flowchart: {{
        useMaxWidth: true,
        htmlLabels: true,
        curve: 'basis'
      }},
      sequence: {{
        useMaxWidth: true,
        fontFamily: 'Inter, sans-serif'
      }}
    }});

    mermaid.run().then(() => {{
      // Render KaTeX Mathematics
      renderMathInElement(document.body, {{
        delimiters: [
          {{ left: '$$', right: '$$', display: true }},
          {{ left: '$', right: '$', display: false }}
        ],
        throwOnError: false
      }});

      document.body.classList.add('rendered-ready');
      console.log('Rendering complete.');
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
        print("Microsoft Edge executable not found. Searching PATH...")
        edge_exe = "msedge.exe"

    print(f"Using browser executable: {edge_exe}")
    print("Executing headless print-to-pdf with compositor and virtual time budget...")

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
        print(f"SUCCESS: Whitepaper PDF created successfully!")
        print(f"File Path: {pdf_path}")
        print(f"Size: {pdf_size_kb:.1f} KB")
    else:
        print("Failed to generate PDF. Stderr:", res.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
