%% IEEEtran.cls compatible reimplementation for compilation
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{IEEEtran}[2024/01/01 IEEE Conference Class]
\DeclareOption{conference}{}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax
\LoadClass[10pt,twocolumn,letterpaper]{article}
\RequirePackage[top=0.75in,bottom=1in,left=0.625in,right=0.625in,columnsep=0.25in]{geometry}
\RequirePackage{times}
\RequirePackage{mathptmx}
\RequirePackage[T1]{fontenc}
\setlength{\parindent}{1em}
\setlength{\parskip}{0pt}
\renewcommand{\baselinestretch}{1.0}
\RequirePackage{titlesec}
\titleformat{\section}{\normalfont\normalsize\bfseries\centering}{\Roman{section}.}{0.5em}{\MakeUppercase}
\titlespacing{\section}{0pt}{9pt plus 2pt minus 2pt}{4pt plus 1pt}
\titleformat{\subsection}{\normalfont\normalsize\itshape}{\Alph{subsection}.}{0.5em}{}
\titlespacing{\subsection}{0pt}{6pt plus 2pt}{3pt}
\newcommand{\IEEEoverridecommandlockouts}{}
\newcommand{\IEEEauthorblockN}[1]{{\large #1}\\[2pt]}
\newcommand{\IEEEauthorblockA}[1]{{\normalsize #1}}
\renewcommand{\maketitle}{%
  \twocolumn[{\begin{center}
    {\LARGE\bfseries \@title \par}\vspace{10pt}
    {\normalsize \@author \par}
  \end{center}\vspace{14pt}}]
  \setcounter{footnote}{0}}
\renewenvironment{abstract}{%
  \begin{center}{\normalsize\bfseries Abstract}\end{center}
  \begin{list}{}{\setlength{\leftmargin}{0.5em}\setlength{\rightmargin}{0.5em}
    \setlength{\itemsep}{0pt}\setlength{\parsep}{0pt}}\item\relax\small
}{\end{list}\vspace{6pt}}
\newenvironment{IEEEkeywords}{%
  \begin{list}{}{\setlength{\leftmargin}{0.5em}\setlength{\rightmargin}{0.5em}}
  \item\relax\small\textit{Index Terms}---\ignorespaces
}{\end{list}\vspace{4pt}}
\RequirePackage{caption}
\DeclareCaptionFormat{ieee}{\small\textbf{#1#2}#3}
\captionsetup{format=ieee,labelsep=period,singlelinecheck=false,skip=4pt,font=small}
\captionsetup[table]{position=top,skip=2pt}
\renewcommand{\topfraction}{0.9}
\renewcommand{\bottomfraction}{0.8}
\setcounter{topnumber}{3}
\setcounter{bottomnumber}{2}
\setcounter{totalnumber}{5}
\renewcommand{\textfraction}{0.07}
\renewcommand{\floatpagefraction}{0.7}
\renewenvironment{thebibliography}[1]{%
  \section*{References}
  \list{[\arabic{enumi}]}{\usecounter{enumi}
    \setlength{\leftmargin}{1.75em}\setlength{\itemsep}{0pt}
    \setlength{\parsep}{0pt}\footnotesize}
}{\endlist}
\endinput
