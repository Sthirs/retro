/**
 * @name Alert suppression
 * @description Generates information about alert suppressions.
 * @kind alert-suppression
 * @id rust/alert-suppression
 */

private import codeql.util.suppression.AlertSuppression as AS
private import rust as R

class AstNode instanceof R::AstNode {
  predicate hasLocationInfo(
    string filepath, int startline, int startcolumn, int endline, int endcolumn
  ) {
    super.getLocation().hasLocationInfo(filepath, startline, startcolumn, endline, endcolumn)
  }

  string toString() { result = super.toString() }
}

class SingleLineComment instanceof R::Comment {
  SingleLineComment() { super.getCommentMarker() = "//" }

  predicate hasLocationInfo(
    string filepath, int startline, int startcolumn, int endline, int endcolumn
  ) {
    super.getLocation().hasLocationInfo(filepath, startline, startcolumn, endline, endcolumn)
  }

  string getText() { result = super.getCommentText() }

  string toString() { result = super.toString() }
}

import AS::Make<AstNode, SingleLineComment>
