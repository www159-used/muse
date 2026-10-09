Feature: Import local lyrics and artwork

  Scenario: Importing local lyrics and a NetEase cover alongside a song
    Given "downloads/song.ncm" is a copy of the NCM fixture "no-cover.ncm"
    And "downloads/meta/track-321.jpg" is a copy of the fixture "cover.png"
    And the text file "downloads/song.lrc" contains:
      """
      [00:00.00]夜空
      [00:01.00]노래
      """

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"
    And the imported track has a PNG cover matching "cover.png"
    And the imported track has these lyrics:
      """
      [00:00.00]夜空
      [00:01.00]노래
      """
