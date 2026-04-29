%% ndss.cls  –  NDSS Symposium conference class (2026/2027 compatible)
%% Based on NDSS formatting requirements:
%%   two-column, US letter, Times 10pt, 13-page body limit,
%%   1-inch top/bottom margins, 0.875-inch left/right margins
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{ndss}[2024/01/01 NDSS Symposium Class]

\DeclareOption{anonymous}{\def\@anonymous{}}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax

\LoadClass[10pt,twocolumn,letterpaper]{article}

%% ── Page geometry ─────────────────────────────────────────────────────────────
\RequirePackage[
  top=1in, bottom=1in,
  left=0.875in, right=0.875in,
  columnsep=0.25in
]{geometry}

%% ── Fonts ─────────────────────────────────────────────────────────────────────
\RequirePackage{times}
\RequirePackage{mathptmx}
\RequirePackage[T1]{fontenc}

%% ── Spacing ───────────────────────────────────────────────────────────────────
\setlength{\parindent}{0.15in}
\setlength{\parskip}{0pt}
\renewcommand{\baselinestretch}{1.0}

%% ── Section headings ──────────────────────────────────────────────────────────
\RequirePackage{titlesec}
\titleformat{\section}
  {\normalfont\normalsize\bfseries\centering}
  {\Roman{section}.}{0.5em}{\MakeUppercase}
\titlespacing{\section}{0pt}{8pt plus 2pt minus 2pt}{4pt plus 1pt}

\titleformat{\subsection}
  {\normalfont\normalsize\bfseries}
  {\Alph{subsection}.}{0.5em}{}
\titlespacing{\subsection}{0pt}{6pt plus 2pt}{3pt}

\titleformat{\subsubsection}
  {\normalfont\normalsize\itshape}
  {\arabic{subsubsection})}{0.5em}{}
\titlespacing{\subsubsection}{0pt}{4pt}{2pt}

%% ── Title block ───────────────────────────────────────────────────────────────
\renewcommand{\maketitle}{%
  \twocolumn[{%
    \begin{center}
      {\Large\bfseries \@title \par}%
      \vspace{8pt}%
      \ifdefined\@anonymous
        {\normalsize\itshape Anonymous Submission — Do Not Distribute \par}%
      \else
        {\normalsize \@author \par}%
      \fi
    \end{center}%
    \vspace{10pt}%
  }]%
  \setcounter{footnote}{0}%
}

%% ── Abstract ──────────────────────────────────────────────────────────────────
\renewenvironment{abstract}{%
  \begin{center}{\normalsize\bfseries Abstract}\end{center}%
  \begin{list}{}{%
    \setlength{\leftmargin}{0.15in}%
    \setlength{\rightmargin}{0.15in}%
    \setlength{\itemsep}{0pt}%
    \setlength{\parsep}{0pt}%
  }%
  \item\relax\small
}{%
  \end{list}%
  \vspace{6pt}%
}

%% ── Keywords ──────────────────────────────────────────────────────────────────
\newenvironment{IEEEkeywords}{%
  \begin{list}{}{%
    \setlength{\leftmargin}{0.15in}%
    \setlength{\rightmargin}{0.15in}%
  }%
  \item\relax\small\textbf{Keywords: }\ignorespaces
}{%
  \end{list}%
  \vspace{4pt}%
}

%% ── Captions ──────────────────────────────────────────────────────────────────
\RequirePackage{caption}
\DeclareCaptionFormat{ndss}{\small\textbf{#1#2}#3}
\captionsetup{format=ndss, labelsep=period, singlelinecheck=false,
              skip=4pt, font=small}
\captionsetup[table]{position=top, skip=2pt}

%% ── Float parameters ──────────────────────────────────────────────────────────
\renewcommand{\topfraction}{0.90}
\renewcommand{\bottomfraction}{0.80}
\setcounter{topnumber}{4}
\setcounter{bottomnumber}{3}
\setcounter{totalnumber}{6}
\renewcommand{\textfraction}{0.07}
\renewcommand{\floatpagefraction}{0.70}

%% ── Bibliography ──────────────────────────────────────────────────────────────
\renewenvironment{thebibliography}[1]{%
  \section*{References}%
  \list{[\arabic{enumi}]}{%
    \usecounter{enumi}%
    \setlength{\leftmargin}{1.5em}%
    \setlength{\itemsep}{2pt}%
    \setlength{\parsep}{0pt}%
    \footnotesize
  }%
}{\endlist}

\endinput
